use jsonschema::Draft;
use std::fmt::{Display, Formatter};
use std::str::FromStr;

pub enum OpenApiVersion {
    V30x,
    V31x,
}

impl FromStr for OpenApiVersion {
    type Err = VersionError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.starts_with("3.1") {
            Ok(OpenApiVersion::V31x)
        } else if s.starts_with("3.0") {
            Ok(OpenApiVersion::V30x)
        } else {
            Err(VersionError::unsupported_version(s))
        }
    }
}

impl OpenApiVersion {
    pub(crate) fn get_draft(&self) -> Draft {
        match self {
            OpenApiVersion::V30x => Draft::Draft4,
            OpenApiVersion::V31x => Draft::Draft202012,
        }
    }
}

#[derive(Debug)]
pub enum VersionError {
    UnsupportedVersion(String),
}

impl VersionError {
    pub(crate) fn unsupported_version<T>(version: &T) -> Self
    where
        T: ToString + ?Sized,
    {
        VersionError::UnsupportedVersion(version.to_string())
    }
}

impl Display for VersionError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            VersionError::UnsupportedVersion(version) => {
                write!(f, "Unsupported version: {}", version)
            }
        }
    }
}

impl std::error::Error for VersionError {}
