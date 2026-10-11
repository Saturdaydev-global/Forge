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
}
