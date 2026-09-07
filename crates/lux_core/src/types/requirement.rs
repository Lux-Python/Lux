use std::fmt;
use std::str::FromStr;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use smol_str::SmolStr;

use super::error::TypeError;
use super::marker::EnvironmentMarker;
use super::package_name::PackageName;
use super::version::Version;

/// Full PEP 508 dependency specification.
///
/// Encapsulates distribution name, extras, version constraints or direct URLs,
/// and conditional environment markers.
#[derive(Clone, PartialEq, Eq)]
pub struct Requirement {
    inner: pep508_rs::Requirement,
    name: PackageName,
    extras: Vec<SmolStr>,
    marker: Option<EnvironmentMarker>,
}

impl Requirement {
    /// Parse a PEP 508 requirement string.
    pub fn parse(s: &str) -> Result<Self, TypeError> {
        let inner = pep508_rs::Requirement::from_str(s)
            .map_err(|source| TypeError::InvalidRequirement {
                requirement: s.to_string(),
                source,
            })?;

        let name = PackageName::new(inner.name.as_ref())?;
        let extras = inner
            .extras
            .iter()
            .map(|extra| SmolStr::new(extra.as_ref()))
            .collect();

        let marker = if inner.marker.is_true() {
            None
        } else {
            Some(EnvironmentMarker::from(inner.marker.clone()))
        };

        Ok(Self {
            inner,
            name,
            extras,
            marker,
        })
    }

    /// The normalized package name for this requirement.
    #[inline]
    pub const fn name(&self) -> &PackageName {
        &self.name
    }

    /// Requested package extras (e.g. `['security', 'socks']`).
    #[inline]
    pub fn extras(&self) -> &[SmolStr] {
        &self.extras
    }

    /// Conditional environment marker, if present.
    #[inline]
    pub const fn marker(&self) -> Option<&EnvironmentMarker> {
        self.marker.as_ref()
    }

    /// Underlying PEP 508 requirement.
    #[inline]
    pub const fn as_pep508(&self) -> &pep508_rs::Requirement {
        &self.inner
    }

    /// Check if a given candidate version satisfies this requirement's version constraint.
    pub fn is_satisfied_by(&self, version: &Version) -> bool {
        match &self.inner.version_or_url {
            Some(pep508_rs::VersionOrUrl::VersionSpecifier(specs)) => {
                specs.contains(version.as_pep440())
            }
            Some(pep508_rs::VersionOrUrl::Url(_)) | None => true,
        }
    }
}

impl FromStr for Requirement {
    type Err = TypeError;

    #[inline]
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

impl fmt::Display for Requirement {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.inner, f)
    }
}

impl fmt::Debug for Requirement {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Requirement(\"{}\")", self.inner)
    }
}

impl Serialize for Requirement {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.inner.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Requirement {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Self::parse(&s).map_err(serde::de::Error::custom)
    }
}
