use std::fmt;
use std::ops::Deref;
use std::str::FromStr;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::error::TypeError;

/// High-performance wrapper around a PEP 440 compliant Python version.
///
/// Implements total ordering, semantic comparison, epoch handling, pre/post/dev releases,
/// and local version identifiers according to PEP 440.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Version(pep440_rs::Version);

impl Version {
    /// Parse a PEP 440 version string.
    ///
    /// # Errors
    /// Returns [`TypeError::InvalidVersion`] if the version string is not valid PEP 440.
    pub fn parse(s: &str) -> Result<Self, TypeError> {
        pep440_rs::Version::from_str(s)
            .map(Self)
            .map_err(|source| TypeError::InvalidVersion {
                version: s.to_string(),
                source,
            })
    }

    /// Access the underlying `pep440_rs::Version`.
    #[inline]
    pub const fn as_pep440(&self) -> &pep440_rs::Version {
        &self.0
    }

    /// Consume into the underlying `pep440_rs::Version`.
    #[inline]
    pub fn into_pep440(self) -> pep440_rs::Version {
        self.0
    }

    /// Returns the epoch number (e.g., in `1!2.0.0`, the epoch is 1). Defaults to 0.
    #[inline]
    pub fn epoch(&self) -> u64 {
        self.0.epoch()
    }

    /// Returns the release segments as a slice of 64-bit unsigned integers.
    #[inline]
    pub fn release(&self) -> &[u64] {
        self.0.release()
    }

    /// Whether this version is a pre-release (e.g. `1.0a1`, `1.0b2`, `1.0rc1`).
    #[inline]
    pub fn is_prerelease(&self) -> bool {
        self.0.pre().is_some()
    }

    /// Whether this version is a post-release (e.g. `1.0.post1`).
    #[inline]
    pub fn is_postrelease(&self) -> bool {
        self.0.is_post()
    }

    /// Whether this version is a development release (e.g. `1.0.dev1`).
    #[inline]
    pub fn is_dev(&self) -> bool {
        self.0.is_dev()
    }

    /// Whether this version contains a local version identifier (e.g. `1.0+cpu`).
    #[inline]
    pub fn is_local(&self) -> bool {
        !self.0.local().is_empty()
    }

    /// Returns the pre-release information, if any.
    #[inline]
    pub fn pre(&self) -> Option<pep440_rs::Prerelease> {
        self.0.pre()
    }

    /// Returns the post-release number, if any.
    #[inline]
    pub fn post(&self) -> Option<u64> {
        self.0.post()
    }

    /// Returns the dev-release number, if any.
    #[inline]
    pub fn dev(&self) -> Option<u64> {
        self.0.dev()
    }

    /// Returns the raw slice of local segments.
    #[inline]
    pub fn local(&self) -> &[pep440_rs::LocalSegment] {
        self.0.local()
    }
}

impl Deref for Version {
    type Target = pep440_rs::Version;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl AsRef<pep440_rs::Version> for Version {
    #[inline]
    fn as_ref(&self) -> &pep440_rs::Version {
        &self.0
    }
}

impl From<pep440_rs::Version> for Version {
    #[inline]
    fn from(v: pep440_rs::Version) -> Self {
        Self(v)
    }
}

impl FromStr for Version {
    type Err = TypeError;

    #[inline]
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

impl fmt::Display for Version {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

impl fmt::Debug for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Version(\"{}\")", self.0)
    }
}

impl Serialize for Version {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.0.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Version {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        pep440_rs::Version::deserialize(deserializer).map(Self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pep440_version_ordering() {
        let v1: Version = "1.0.dev1".parse().expect("valid version");
        let v2: Version = "1.0a1".parse().expect("valid version");
        let v3: Version = "1.0b1".parse().expect("valid version");
        let v4: Version = "1.0rc1".parse().expect("valid version");
        let v5: Version = "1.0".parse().expect("valid version");
        let v6: Version = "1.0.post1".parse().expect("valid version");
        let v7: Version = "1.1".parse().expect("valid version");
        let v8: Version = "1!0.1".parse().expect("epoch version");

        assert!(v1 < v2);
        assert!(v2 < v3);
        assert!(v3 < v4);
        assert!(v4 < v5);
        assert!(v5 < v6);
        assert!(v6 < v7);
        assert!(v7 < v8);
    }

    #[test]
    fn test_version_components() {
        let v: Version = "2!1.2.3b4.post5.dev6+local.build1".parse().expect("valid version");

        assert_eq!(v.epoch(), 2);
        assert_eq!(v.release(), &[1, 2, 3]);
        assert!(v.is_prerelease());
        assert!(v.is_postrelease());
        assert!(v.is_dev());
        assert!(v.is_local());
        assert_eq!(v.post(), Some(5));
        assert_eq!(v.dev(), Some(6));
        assert_eq!(v.local().len(), 2);
    }

    #[test]
    fn test_invalid_versions() {
        assert!("not-a-version".parse::<Version>().is_err());
        assert!("1.2.3.4.5.x".parse::<Version>().is_err());
        assert!("".parse::<Version>().is_err());
    }

    #[test]
    fn test_serde_version() {
        let v: Version = "3.11.4".parse().expect("valid");
        let json = serde_json::to_string(&v).expect("serialize");
        let deserialized: Version = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(v, deserialized);
    }
}
