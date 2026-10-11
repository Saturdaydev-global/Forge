use crate::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectName(String);

impl ProjectName {
    pub fn new(value: &str) -> Result<Self, Error> {
        let value = value.trim();
        if value.is_empty() || value.chars().count() > 200 || value.chars().any(char::is_control) {
            return Err(Error::InvalidProjectName);
        }
        Ok(Self(value.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct Project {
    pub id: i64,
    pub name: ProjectName,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskTitle(String);

impl TaskTitle {
    pub fn new(value: &str) -> Result<Self, Error> {
        let value = value.trim();
        if value.is_empty() || value.chars().count() > 200 || value.chars().any(char::is_control) {
            return Err(Error::InvalidTaskTitle);
        }
        Ok(Self(value.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Criterion(String);

impl Criterion {
    pub fn new(value: &str) -> Result<Self, Error> {
        let value = value.trim();
        if value.is_empty() || value.chars().count() > 200 || value.chars().any(char::is_control) {
            return Err(Error::InvalidCriterion);
        }
        Ok(Self(value.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskStatus {
    Backlog,
    Ready,
    Running,
    Review,
    Completed,
    Blocked,
    Failed,
    Cancelled,
}

impl TaskStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Backlog => "backlog",
            Self::Ready => "ready",
            Self::Running => "running",
            Self::Review => "review",
            Self::Completed => "completed",
            Self::Blocked => "blocked",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }

    pub fn can_change_to(self, next: Self) -> bool {
        use TaskStatus::*;
        matches!(
            (self, next),
            (Backlog | Blocked | Failed | Cancelled, Ready)
                | (Ready, Backlog | Blocked | Cancelled)
                | (Backlog | Blocked | Failed, Cancelled)
                | (Review, Completed | Ready | Cancelled)
        )
    }

    pub fn dependencies_editable(self) -> bool {
        !matches!(self, Self::Running | Self::Review | Self::Completed)
    }
}

impl std::str::FromStr for TaskStatus {
    type Err = Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "backlog" => Ok(Self::Backlog),
            "ready" => Ok(Self::Ready),
            "running" => Ok(Self::Running),
            "review" => Ok(Self::Review),
            "completed" => Ok(Self::Completed),
            "blocked" => Ok(Self::Blocked),
            "failed" => Ok(Self::Failed),
            "cancelled" => Ok(Self::Cancelled),
            _ => Err(Error::InvalidTaskStatus),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct Task {
    pub id: i64,
    pub project_id: i64,
    pub title: TaskTitle,
    pub status: TaskStatus,
    pub criteria: Vec<Criterion>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_and_normalizes_names() {
        assert_eq!(ProjectName::new("  Forge  ").unwrap().as_str(), "Forge");
        assert!(ProjectName::new(&"é".repeat(200)).is_ok());
        for value in ["", "  ", "a\nb", "a\0b", &"x".repeat(201)] {
            assert!(matches!(
                ProjectName::new(value),
                Err(Error::InvalidProjectName)
            ));
        }
    }

    #[test]
    fn validates_task_titles_and_criteria() {
        assert_eq!(TaskTitle::new("  Routing  ").unwrap().as_str(), "Routing");
        assert_eq!(Criterion::new("  Done  ").unwrap().as_str(), "Done");
        for value in ["", "  ", "a\nb", &"x".repeat(201)] {
            assert!(matches!(
                TaskTitle::new(value),
                Err(Error::InvalidTaskTitle)
            ));
            assert!(matches!(
                Criterion::new(value),
                Err(Error::InvalidCriterion)
            ));
        }
        assert_eq!(
            "backlog".parse::<TaskStatus>().unwrap(),
            TaskStatus::Backlog
        );
        assert!("done".parse::<TaskStatus>().is_err());
    }
}
