use std::borrow::Borrow;
use std::fmt;
use std::ops::Deref;
use std::str::FromStr;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use smol_str::SmolStr;

use super::error::TypeError;

/// Zero-copy, interned Python package name following PEP 503 and PEP 508 normalization rules.
///
/// Backed by [`SmolStr`] to guarantee zero-heap allocation for short names (<= 23 bytes),
/// which covers >99% of Python distribution packages.
#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct PackageName(SmolStr);

impl PackageName {
    /// Create a new validated, PEP 503 normalized package name.
    ///
    /// # Errors
    /// Returns [`TypeError::InvalidPackageName`] if the name violates PEP 508 format rules.
    pub fn new(raw: &str) -> Result<Self, TypeError> {
        validate_package_name(raw)?;
        let normalized = normalize_name(raw);
        Ok(Self(SmolStr::new(normalized)))
    }

    /// Create a package name without validation or normalization.
    ///
    /// # Safety / Invariant
    /// The caller must ensure that the string is already PEP 503 normalized and valid PEP 508.
    pub const fn from_normalized_unchecked(normalized: SmolStr) -> Self {
        Self(normalized)
    }

    /// Returns the underlying normalized string slice.
    #[inline]
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }

    /// Returns the underlying [`SmolStr`].
    #[inline]
    pub const fn as_smol_str(&self) -> &SmolStr {
        &self.0
    }
}

/// Normalizes a package name according to PEP 503:
/// Convert to lowercase and replace any sequence of `[._-]+` with a single `-`.
pub fn normalize_name(name: &str) -> String {
    let mut normalized = String::with_capacity(name.len());
    let mut last_was_punct = false;

    for ch in name.chars() {
        if ch == '.' || ch == '_' || ch == '-' {
            if !last_was_punct {
                normalized.push('-');
                last_was_punct = true;
            }
        } else {
            normalized.push(ch.to_ascii_lowercase());
            last_was_punct = false;
        }
    }

    normalized
}

/// Validates that a raw string adheres to the PEP 508 distribution name specification:
/// `^([A-Z0-9]|[A-Z0-9][A-Z0-9._-]*[A-Z0-9])$` (case-insensitive)
fn validate_package_name(name: &str) -> Result<(), TypeError> {
    if name.is_empty() {
        return Err(TypeError::InvalidPackageName {
            name: name.to_string(),
            reason: "Package name cannot be empty".to_string(),
        });
    }

    let first = name.chars().next().unwrap_or('\0');
    if !first.is_ascii_alphanumeric() {
        return Err(TypeError::InvalidPackageName {
            name: name.to_string(),
            reason: "Package name must start with an ASCII alphanumeric character".to_string(),
        });
    }

    let last = name.chars().last().unwrap_or('\0');
    if !last.is_ascii_alphanumeric() {
        return Err(TypeError::InvalidPackageName {
            name: name.to_string(),
            reason: "Package name must end with an ASCII alphanumeric character".to_string(),
        });
    }

    for ch in name.chars() {
        if !ch.is_ascii_alphanumeric() && ch != '.' && ch != '_' && ch != '-' {
            return Err(TypeError::InvalidPackageName {
                name: name.to_string(),
                reason: format!("Illegal character in package name: '{ch}'"),
            });
        }
    }

    Ok(())
}

impl Deref for PackageName {
    type Target = str;

    #[inline]
    fn deref(&self) -> &Self::Target {
        self.as_str()
    }
}

impl AsRef<str> for PackageName {
    #[inline]
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl Borrow<str> for PackageName {
    #[inline]
    fn borrow(&self) -> &str {
        self.as_str()
    }
}

impl fmt::Display for PackageName {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl fmt::Debug for PackageName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "PackageName(\"{}\")", self.as_str())
    }
}

impl FromStr for PackageName {
    type Err = TypeError;

    #[inline]
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::new(s)
    }
}

impl Serialize for PackageName {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for PackageName {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Self::new(&s).map_err(serde::de::Error::custom)
    }
}

impl TryFrom<pep508_rs::PackageName> for PackageName {
    type Error = TypeError;

    fn try_from(pkg: pep508_rs::PackageName) -> Result<Self, Self::Error> {
        Self::new(pkg.as_ref())
    }
}

impl TryFrom<&pep508_rs::PackageName> for PackageName {
    type Error = TypeError;

    fn try_from(pkg: &pep508_rs::PackageName) -> Result<Self, Self::Error> {
        Self::new(pkg.as_ref())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pep503_normalization() {
        assert_eq!(normalize_name("foo_bar"), "foo-bar");
        assert_eq!(normalize_name("foo.bar"), "foo-bar");
        assert_eq!(normalize_name("foo--bar"), "foo-bar");
        assert_eq!(normalize_name("foo__bar"), "foo-bar");
        assert_eq!(normalize_name("Foo.Bar_Baz"), "foo-bar-baz");
        assert_eq!(normalize_name("PyTorch"), "pytorch");
    }

    #[test]
    fn test_package_name_creation() {
        let pkg1 = PackageName::new("Torch_Geometric").expect("valid package name");
        let pkg2 = PackageName::new("torch-geometric").expect("valid package name");
        let pkg3 = PackageName::new("TORCH.GEOMETRIC").expect("valid package name");

        assert_eq!(pkg1, pkg2);
        assert_eq!(pkg2, pkg3);
        assert_eq!(pkg1.as_str(), "torch-geometric");
    }

    #[test]
    fn test_invalid_package_names() {
        assert!(PackageName::new("").is_err());
        assert!(PackageName::new("-invalid").is_err());
        assert!(PackageName::new("invalid-").is_err());
        assert!(PackageName::new(".invalid").is_err());
        assert!(PackageName::new("inv@lid").is_err());
    }

    #[test]
    fn test_serde_roundtrip() {
        let name = PackageName::new("Scikit-Learn").expect("valid");
        let json = serde_json::to_string(&name).expect("serialize");
        assert_eq!(json, "\"scikit-learn\"");

        let deserialized: PackageName = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(deserialized, name);
    }
}
