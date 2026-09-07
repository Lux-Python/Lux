use std::fmt;
use std::ops::Deref;
use std::str::FromStr;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::error::TypeError;
use super::version::Version;

/// Single PEP 440 version specifier clause (e.g., `>= 1.20.0`).
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct VersionSpecifier(pep440_rs::VersionSpecifier);

impl VersionSpecifier {
    /// Parse a single version specifier.
    pub fn parse(s: &str) -> Result<Self, TypeError> {
        pep440_rs::VersionSpecifier::from_str(s)
            .map(Self)
            .map_err(|source| TypeError::InvalidVersionSpecifier {
                spec: s.to_string(),
                source,
            })
    }

    /// Access the underlying `pep440_rs::VersionSpecifier`.
    #[inline]
    pub const fn as_pep440(&self) -> &pep440_rs::VersionSpecifier {
        &self.0
    }

    /// Check if a version satisfies this individual specifier clause.
    #[inline]
    pub fn contains(&self, version: &Version) -> bool {
        self.0.contains(version.as_pep440())
    }

    /// The comparison operator for this clause (e.g., `>=`, `<`).
    #[inline]
    pub fn operator(&self) -> pep440_rs::Operator {
        *self.0.operator()
    }

    /// The target version for this clause.
    #[inline]
    pub fn version(&self) -> &pep440_rs::Version {
        self.0.version()
    }
}

impl Deref for VersionSpecifier {
    type Target = pep440_rs::VersionSpecifier;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl FromStr for VersionSpecifier {
    type Err = TypeError;

    #[inline]
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

impl fmt::Display for VersionSpecifier {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

impl fmt::Debug for VersionSpecifier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "VersionSpecifier(\"{}\")", self.0)
    }
}

impl Serialize for VersionSpecifier {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.0.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for VersionSpecifier {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        pep440_rs::VersionSpecifier::deserialize(deserializer).map(Self)
    }
}

/// A compound set of PEP 440 version specifiers (e.g. `>= 1.20.0, < 2.0.0, != 1.24.0`).
#[derive(Clone, Default, PartialEq, Eq, Hash)]
pub struct VersionSpecifiers(pep440_rs::VersionSpecifiers);

impl VersionSpecifiers {
    /// An empty set of specifiers, representing matching any version (`*`).
    #[inline]
    pub fn empty() -> Self {
        Self(pep440_rs::VersionSpecifiers::empty())
    }

    /// Parse a compound version specifier string.
    pub fn parse(s: &str) -> Result<Self, TypeError> {
        if s.trim().is_empty() || s.trim() == "*" {
            return Ok(Self::empty());
        }

        pep440_rs::VersionSpecifiers::from_str(s)
            .map(Self)
            .map_err(|source| TypeError::InvalidVersionSpecifiers {
                spec: s.to_string(),
                source,
            })
    }

    /// Whether this specifier set is empty (accepts all versions).
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Number of specifier clauses.
    #[inline]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Check if a version satisfies all clauses in this specifier set.
    #[inline]
    pub fn contains(&self, version: &Version) -> bool {
        self.0.contains(version.as_pep440())
    }

    /// Access the underlying `pep440_rs::VersionSpecifiers`.
    #[inline]
    pub const fn as_pep440(&self) -> &pep440_rs::VersionSpecifiers {
        &self.0
    }
}

impl Deref for VersionSpecifiers {
    type Target = pep440_rs::VersionSpecifiers;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl FromStr for VersionSpecifiers {
    type Err = TypeError;

    #[inline]
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

impl fmt::Display for VersionSpecifiers {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

impl fmt::Debug for VersionSpecifiers {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "VersionSpecifiers(\"{}\")", self.0)
    }
}

impl Serialize for VersionSpecifiers {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.0.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for VersionSpecifiers {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        pep440_rs::VersionSpecifiers::deserialize(deserializer).map(Self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compound_version_specifiers() {
        let specs: VersionSpecifiers = ">= 1.20.0, < 2.0.0, != 1.25.0".parse().expect("valid specifiers");

        let v_ok: Version = "1.24.1".parse().unwrap();
        let v_excluded: Version = "1.25.0".parse().unwrap();
        let v_too_low: Version = "1.19.9".parse().unwrap();
        let v_too_high: Version = "2.0.0".parse().unwrap();

        assert!(specs.contains(&v_ok));
        assert!(!specs.contains(&v_excluded));
        assert!(!specs.contains(&v_too_low));
        assert!(!specs.contains(&v_too_high));
    }

    #[test]
    fn test_tilde_equal_compatibility() {
        let specs: VersionSpecifiers = "~= 1.4.1".parse().expect("valid ~= specifier");

        let v1: Version = "1.4.1".parse().unwrap();
        let v2: Version = "1.4.9".parse().unwrap();
        let v3: Version = "1.5.0".parse().unwrap();

        assert!(specs.contains(&v1));
        assert!(specs.contains(&v2));
        assert!(!specs.contains(&v3));
    }

    #[test]
    fn test_empty_specifier() {
        let empty1: VersionSpecifiers = "".parse().unwrap();
        let empty2: VersionSpecifiers = "*".parse().unwrap();
        let any_ver: Version = "99.0.0".parse().unwrap();

        assert!(empty1.contains(&any_ver));
        assert!(empty2.contains(&any_ver));
    }
}
