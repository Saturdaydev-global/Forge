use crate::{Error, storage::Store};
use rusqlite::params;
use std::io::Read;

pub const FREE_ROUTER_CATALOG: &str =
    "https://raw.githubusercontent.com/humantonylee/free-router/main/data/model-rankings.json";

#[derive(Clone, Debug)]
pub struct Model {
    pub id: String,
    pub cost: String,
    pub access: String,
    pub source: String,
    pub enabled: bool,
    pub priority: i64,
}

pub fn valid_model(id: &str) -> bool {
    id.contains('/')
        && id.len() <= 200
        && id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"/._:-".contains(&c))
}

impl Store {
    pub fn register_model(&self, model: &Model) -> Result<(), Error> {
        if !valid_model(&model.id)
            || !["free", "subscription", "paid", "unknown"].contains(&model.cost.as_str())
            || !["anonymous", "subscription", "api-key", "unknown"].contains(&model.access.as_str())
        {
            return Err(Error::Policy(
                "invalid provider/model, cost or access classification".into(),
            ));
        }
        self.connection.execute("INSERT INTO models(id,cost,access,source,enabled,priority) VALUES (?1,?2,?3,'manual',?4,?5) ON CONFLICT(id) DO UPDATE SET cost=excluded.cost,access=excluded.access,source='manual',enabled=excluded.enabled,priority=excluded.priority,updated_at=unixepoch()",
            params![model.id,model.cost,model.access,model.enabled,model.priority])?;
        Ok(())
    }

    pub fn models(&self) -> Result<Vec<Model>, Error> {
        let mut statement = self.connection.prepare(
            "SELECT id,cost,access,source,enabled,priority FROM models ORDER BY priority DESC,id",
        )?;
        Ok(statement
            .query_map([], |r| {
                Ok(Model {
                    id: r.get(0)?,
                    cost: r.get(1)?,
                    access: r.get(2)?,
                    source: r.get(3)?,
                    enabled: r.get(4)?,
                    priority: r.get(5)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?)
    }

    pub fn route(&self, policy: &str, pinned: Option<&str>) -> Result<Model, Error> {
        let mut models: Vec<_> = self
            .models()?
            .into_iter()
            .filter(|m| m.enabled && ["free", "subscription"].contains(&m.cost.as_str()))
            .collect();
        match policy {
            "pinned" => models.retain(|m| Some(m.id.as_str()) == pinned),
            "free" | "economy" => models.retain(|m| m.cost == "free"),
            "subscription-first" => models.sort_by_key(|m| m.cost != "subscription"),
            "balanced" | "best" => {}
            _ => return Err(Error::Policy("unknown routing policy".into())),
        }
        models.into_iter().next().ok_or_else(|| {
            Error::Policy(
                "no enabled zero-new-spend model matches; paid/unknown routes are excluded".into(),
            )
        })
    }

    pub fn import_free_router(&mut self, text: &str) -> Result<usize, Error> {
        let document: serde_json::Value = serde_json::from_str(text)
            .map_err(|_| Error::Policy("invalid Free Router catalog JSON".into()))?;
        let entries = document["models"]
            .as_array()
            .ok_or_else(|| Error::Policy("Free Router catalog has no models array".into()))?;
        if entries.len() > 10000 {
            return Err(Error::Policy("catalog is too large".into()));
        }
        let tx = self.connection.transaction()?;
        let mut count = 0;
        for item in entries {
            let Some(id) = item["model_id"].as_str() else {
                continue;
            };
            let source = item["source"].as_str().unwrap_or("unknown");
            let provider = match source {
                "openrouter" => "openrouter",
                "nvidia" | "nim" => "nvidia",
                _ => continue,
            };
            let full = format!("{provider}/{id}");
            if !valid_model(&full) {
                continue;
            }
            count+=tx.execute("INSERT OR IGNORE INTO models(id,cost,access,source) VALUES (?1,'unknown','api-key','free-router:discovered')",[full])?;
        }
        tx.commit()?;
        Ok(count)
    }
}

pub fn discover(store: &mut Store) -> Result<usize, Error> {
    let agent = ureq::Agent::config_builder()
        .timeout_global(Some(std::time::Duration::from_secs(20)))
        .build()
        .new_agent();
    let mut response = agent.get(FREE_ROUTER_CATALOG).call().map_err(|_| {
        Error::Policy("Free Router catalog fetch failed; existing registry preserved".into())
    })?;
    let mut text = String::new();
    response
        .body_mut()
        .as_reader()
        .take(1_048_577)
        .read_to_string(&mut text)?;
    if text.len() > 1_048_576 {
        return Err(Error::Policy("catalog exceeds 1 MiB".into()));
    }
    store.import_free_router(&text)
}
