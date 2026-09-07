use std::fmt;
use std::str::FromStr;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::error::TypeError;

/// PEP 508 Environment Marker evaluation tree.
///
/// Encapsulates Boolean logic trees (`and`, `or`, `in`, `not in`, `==`, `!=`, `<`, `<=`, `>`, `>=`)
/// over Python runtime properties like `os_name`, `sys_platform`, `python_version`, and `extra`.
#[derive(Clone, PartialEq, Eq)]
pub struct EnvironmentMarker(pep508_rs::MarkerTree);

impl EnvironmentMarker {
    /// Parse a PEP 508 environment marker string.
    pub fn parse(s: &str) -> Result<Self, TypeError> {
        pep508_rs::MarkerTree::from_str(s)
            .map(Self)
            .map_err(|source| TypeError::InvalidEnvironmentMarker {
                marker: s.to_string(),
                source,
            })
    }

    /// Access the underlying [`pep508_rs::MarkerTree`].
    #[inline]
    pub const fn as_marker_tree(&self) -> &pep508_rs::MarkerTree {
        &self.0
    }

    /// True if the marker is unconditionally satisfied (empty / tautology).
    #[inline]
    pub fn is_tautology(&self) -> bool {
        self.0.is_true()
    }

    /// True if the marker is unsatisfiable (contradiction).
    #[inline]
    pub fn is_contradiction(&self) -> bool {
        self.0.is_false()
    }

    /// Evaluate this marker against an environment and active extras.
    pub fn evaluate(&self, env: &pep508_rs::MarkerEnvironment, extras: &[&str]) -> bool {
        let parsed_extras: Vec<pep508_rs::ExtraName> = extras
            .iter()
            .filter_map(|e| pep508_rs::ExtraName::from_str(e).ok())
            .collect();
        self.0.evaluate(env, &parsed_extras)
    }

    /// Evaluate without any extra feature active.
    pub fn evaluate_no_extra(&self, env: &pep508_rs::MarkerEnvironment) -> bool {
        self.0.evaluate(env, &[])
    }
}

impl From<pep508_rs::MarkerTree> for EnvironmentMarker {
    #[inline]
    fn from(tree: pep508_rs::MarkerTree) -> Self {
        Self(tree)
    }
}

impl FromStr for EnvironmentMarker {
    type Err = TypeError;

    #[inline]
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

impl fmt::Display for EnvironmentMarker {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.contents().map_or(Ok(()), |contents| write!(f, "{contents}"))
    }
}

impl fmt::Debug for EnvironmentMarker {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(contents) = self.0.contents() {
            write!(f, "EnvironmentMarker(\"{contents}\")")
        } else {
            write!(f, "EnvironmentMarker(<true>)")
        }
    }
}

impl Serialize for EnvironmentMarker {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for EnvironmentMarker {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Self::parse(&s).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_marker_parsing_and_display() {
        let marker = EnvironmentMarker::parse("os_name == 'posix' and python_version >= '3.8'").unwrap();
        assert!(!marker.is_tautology());
        assert!(!marker.is_contradiction());
        assert!(!marker.to_string().is_empty());
    }
}
