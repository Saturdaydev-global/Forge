use crate::{
    Error,
    domain::Task,
    execution::{Outcome, WorkerGuard},
    providers::Model,
    storage::Store,
};
use serde_json::{Value, json};
use std::{
    io::Read,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

pub trait Backend {
    fn name(&self) -> &str;
    fn model(&self) -> &str;
    fn execute(&self, task: &Task, store: &mut Store, run: i64) -> Result<Outcome, Error>;
}

pub fn execute(db: &Path, task_id: i64, backend: &impl Backend) -> Result<i64, Error> {
    let mut store = Store::open(db)?;
    let guard = WorkerGuard::acquire(db)?;
    let task = store.get_task(task_id)?;
    let id = store.begin_run(task_id, backend.name(), backend.model(), &guard)?;
    match backend.execute(&task, &mut store, id) {
        Ok(outcome) => {
            store.finish_run(id, outcome)?;
            if outcome != Outcome::Succeeded {
                return Err(Error::Policy(format!(
                    "run {id} ended {}",
                    outcome.as_str()
                )));
            }
        }
        Err(error) => {
            store.finish_run(id, Outcome::Failed)?;
            return Err(error);
        }
    }
    Ok(id)
}

pub struct OpenCode {
    executable: PathBuf,
    model: String,
    timeout: Duration,
}

struct ManagedChild(std::process::Child);
impl Drop for ManagedChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

impl OpenCode {
    pub fn new(executable: PathBuf, model: &Model, timeout: Duration) -> Result<Self, Error> {
        if !model.enabled
            || model.cost != "free"
            || model.access != "anonymous"
            || !model.id.starts_with("opencode/")
            || !model.id.ends_with("-free")
        {
            return Err(Error::Policy("this adapter only permits explicitly enabled anonymous OpenCode *-free models; subscription/API-key execution awaits credential grants".into()));
        }
        if timeout.is_zero() || timeout > Duration::from_secs(600) {
            return Err(Error::Policy("timeout must be 1–600 seconds".into()));
        }
        Ok(Self {
            executable,
            model: model.id.clone(),
            timeout,
        })
    }

    fn command(&self, root: &Path, prompt: &str) -> Command {
        let mut command = Command::new(&self.executable);
        command.env_clear();
        for key in [
            "PATH",
            "SystemRoot",
            "WINDIR",
            "COMSPEC",
            "PATHEXT",
            "TEMP",
            "TMP",
        ] {
            if let Some(value) = std::env::var_os(key) {
                command.env(key, value);
            }
        }
        for key in [
            "HOME",
            "USERPROFILE",
            "XDG_CONFIG_HOME",
            "XDG_DATA_HOME",
            "XDG_CACHE_HOME",
            "XDG_STATE_HOME",
            "APPDATA",
            "LOCALAPPDATA",
            "OPENCODE_CONFIG_DIR",
        ] {
            command.env(key, root.join(key));
        }
        let policy = json!({"$schema":"https://opencode.ai/config.json", "model":self.model,"small_model":self.model,"enabled_providers":["opencode"],"autoupdate":false,"share":"disabled","permission":{"*":"deny"},"agent":{"forge":{"mode":"primary","description":"Forge code proposal without tool access","permission":{"*":"deny"}}},"formatter":false,"lsp":false});
        command
            .env("OPENCODE_CONFIG_CONTENT", policy.to_string())
            .env("OPENCODE_DISABLE_AUTOUPDATE", "true")
            .current_dir(root)
            .args([
                "run",
                "--pure",
                "--format",
                "json",
                "--model",
                &self.model,
                "--agent",
                "forge",
                "--",
                prompt,
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000);
        }
        command
    }
}

impl Backend for OpenCode {
    fn name(&self) -> &str {
        "opencode"
    }
    fn model(&self) -> &str {
        &self.model
    }
    fn execute(&self, task: &Task, store: &mut Store, run: i64) -> Result<Outcome, Error> {
        let workspace = tempfile::tempdir()?;
        let prompt = format!(
            "Produce a code proposal as text for this task. Do not use tools. Do not claim tests ran.\nTask: {}\nAcceptance criteria:\n{}",
            task.title.as_str(),
            task.criteria
                .iter()
                .map(|c| c.as_str())
                .collect::<Vec<_>>()
                .join("\n")
        );
        let mut managed = ManagedChild(self.command(workspace.path(), &prompt).spawn()?);
        let tree = crate::process::ProcessTree::attach(&managed.0)?;
        let child = &mut managed.0;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| Error::Policy("missing backend stdout".into()))?;
        let reader = thread::spawn(move || {
            let mut data = Vec::new();
            let result = stdout.take(1_048_577).read_to_end(&mut data);
            (result, data)
        });
        let started = Instant::now();
        let outcome = loop {
            if store.cancellation_requested(run)? {
                let _ = child.kill();
                break Outcome::Cancelled;
            }
            if started.elapsed() >= self.timeout {
                let _ = child.kill();
                break Outcome::TimedOut;
            }
            if let Some(status) = child.try_wait()? {
                break if status.success() {
                    Outcome::Succeeded
                } else {
                    Outcome::Failed
                };
            }
            thread::sleep(Duration::from_millis(100));
        };
        child.wait()?;
        drop(tree);
        let (result, data) = reader
            .join()
            .map_err(|_| Error::Policy("backend output reader failed".into()))?;
        result?;
        if outcome != Outcome::Succeeded {
            return Ok(outcome);
        }
        let proposal = parse_events(&data)?;
        store.record_result(run, &proposal)?;
        Ok(Outcome::Succeeded)
    }
}

pub fn parse_events(data: &[u8]) -> Result<String, Error> {
    if data.len() > 1_048_576 {
        return Err(Error::Policy("backend output exceeded 1 MiB".into()));
    }
    let mut proposal = String::new();
    let mut finished = false;
    for line in data.split(|b| *b == b'\n').filter(|l| !l.is_empty()) {
        let value: Value = serde_json::from_slice(line)
            .map_err(|_| Error::Policy("backend returned invalid JSON events".into()))?;
        match value["type"].as_str() {
            Some("error") => {
                return Err(Error::Policy(
                    "backend reported an error; raw provider diagnostics withheld".into(),
                ));
            }
            Some("text") => {
                if let Some(text) = value["part"]["text"].as_str() {
                    proposal.push_str(text);
                    proposal.push('\n');
                }
            }
            Some("step_finish") if value["part"]["reason"].as_str() == Some("stop") => {
                finished = true;
            }
            _ => {}
        }
    }
    if !finished || proposal.trim().is_empty() || proposal.len() > 65_536 {
        return Err(Error::Policy(
            "backend did not produce a complete bounded proposal".into(),
        ));
    }
    Ok(proposal)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn child_receives_only_allowlisted_environment_and_denied_tools() {
        let model = Model {
            id: "opencode/test-free".into(),
            cost: "free".into(),
            access: "anonymous".into(),
            source: "manual".into(),
            enabled: true,
            priority: 0,
        };
        let backend = OpenCode::new("opencode".into(), &model, Duration::from_secs(5)).unwrap();
        let tmp = tempfile::tempdir().unwrap();
        let command = backend.command(tmp.path(), "synthetic public input");
        let vars = command.get_envs().collect::<Vec<_>>();
        assert!(
            vars.iter()
                .all(|(key, _)| !key.to_string_lossy().contains("API_KEY"))
        );
        let config = vars
            .iter()
            .find(|(key, _)| *key == "OPENCODE_CONFIG_CONTENT")
            .unwrap()
            .1
            .unwrap()
            .to_str()
            .unwrap();
        let value: Value = serde_json::from_str(config).unwrap();
        assert_eq!(value["permission"]["*"], "deny");
        assert_eq!(value["small_model"], model.id);
        assert_eq!(value["share"], "disabled");
    }
}
