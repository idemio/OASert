use crate::error::{Section, SpecificationSection, ValidationErrorType};
use jsonschema::Draft;
use std::fmt::{Display, Formatter};
use std::str::FromStr;

pub enum OpenApiVersion {
    V30x,
    V31x,
}

impl FromStr for OpenApiVersion {
    type Err = ValidationErrorType;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.starts_with("3.1") {
            Ok(OpenApiVersion::V31x)
        } else if s.starts_with("3.0") {
            Ok(OpenApiVersion::V30x)
        } else {
            Err(ValidationErrorType::UnsupportedSpecVersion(
                s.to_string(),
                Section::Specification(SpecificationSection::Other),
            ))
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
pub(crate) enum VersionError<'a> {
    UnknownVersion(&'a str),
    UnsupportedVersion(&'a str),
}

impl Display for VersionError<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            VersionError::UnknownVersion(version) => {
                write!(f, "Unknown version: {}", version)
            }
            VersionError::UnsupportedVersion(version) => {
                write!(f, "Unsupported version: {}", version)
            }
        }
    }
}

impl std::error::Error for VersionError<'_> {}
