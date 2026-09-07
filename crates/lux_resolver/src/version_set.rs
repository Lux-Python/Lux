use std::fmt;
use std::ops::Bound;
use lux_core::types::{Version, VersionSpecifiers};
use version_ranges::Ranges;

/// Mathematical set of PEP 440 versions, represented as a canonical sequence of disjoint intervals.
///
/// Underpins `PubGrub` proposition testing, boolean algebra, and CDCL learning.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct VersionSet(Ranges<pep440_rs::Version>);

impl VersionSet {
    /// The universal set containing every possible version.
    #[inline]
    #[must_use]
    pub fn full() -> Self {
        Self(Ranges::full())
    }

    /// The empty set containing no versions.
    #[inline]
    #[must_use]
    pub fn empty() -> Self {
        Self(Ranges::empty())
    }

    /// A singleton set containing exactly one version.
    #[inline]
    #[must_use]
    pub fn singleton(version: &Version) -> Self {
        Self(Ranges::singleton(version.as_pep440().clone()))
    }

    /// Construct a version set from a [`VersionSpecifiers`].
    #[must_use]
    pub fn from_specifiers(specifiers: &VersionSpecifiers) -> Self {
        Self(Ranges::from(specifiers.as_pep440().clone()))
    }

    /// Test if a specific version is contained within this set.
    #[inline]
    #[must_use]
    pub fn contains(&self, version: &Version) -> bool {
        self.0.contains(version.as_pep440())
    }

    /// Intersection of two version sets (A ∩ B).
    #[inline]
    #[must_use]
    pub fn intersection(&self, other: &Self) -> Self {
        Self(self.0.intersection(&other.0))
    }

    /// Union of two version sets (A ∪ B).
    #[inline]
    #[must_use]
    pub fn union(&self, other: &Self) -> Self {
        Self(self.0.union(&other.0))
    }

    /// Complement of this version set (complement: U \ A).
    #[inline]
    #[must_use]
    pub fn complement(&self) -> Self {
        Self(self.0.complement())
    }

    /// Difference of two version sets (A \ B = A ∩ complement(B)).
    #[inline]
    #[must_use]
    pub fn difference(&self, other: &Self) -> Self {
        Self(self.0.intersection(&other.0.complement()))
    }

    /// Whether this set contains no versions.
    #[inline]
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Whether this set contains all versions.
    #[inline]
    #[must_use]
    pub fn is_full(&self) -> bool {
        self.0 == Ranges::full()
    }

    /// Whether this set has no overlap with another set (A ∩ B = ∅).
    #[inline]
    #[must_use]
    pub fn is_disjoint(&self, other: &Self) -> bool {
        self.0.is_disjoint(&other.0)
    }

    /// Access the underlying [`Ranges`].
    #[inline]
    #[must_use]
    pub const fn as_ranges(&self) -> &Ranges<pep440_rs::Version> {
        &self.0
    }
}

impl Default for VersionSet {
    #[inline]
    fn default() -> Self {
        Self::full()
    }
}

impl From<Ranges<pep440_rs::Version>> for VersionSet {
    #[inline]
    fn from(ranges: Ranges<pep440_rs::Version>) -> Self {
        Self(ranges)
    }
}

impl fmt::Display for VersionSet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_full() {
            return write!(f, "*");
        }
        if self.is_empty() {
            return write!(f, "<none>");
        }

        let segments: Vec<String> = self
            .0
            .iter()
            .map(|(start, end)| match (start, end) {
                (Bound::Included(s), Bound::Included(e)) if s == e => format!("=={s}"),
                (Bound::Included(s), Bound::Included(e)) => format!(">={s}, <={e}"),
                (Bound::Included(s), Bound::Excluded(e)) => format!(">={s}, <{e}"),
                (Bound::Excluded(s), Bound::Included(e)) => format!(">{s}, <={e}"),
                (Bound::Excluded(s), Bound::Excluded(e)) => format!(">{s}, <{e}"),
                (Bound::Included(s), Bound::Unbounded) => format!(">={s}"),
                (Bound::Excluded(s), Bound::Unbounded) => format!(">{s}"),
                (Bound::Unbounded, Bound::Included(e)) => format!("<={e}"),
                (Bound::Unbounded, Bound::Excluded(e)) => format!("<{e}"),
                (Bound::Unbounded, Bound::Unbounded) => "*".to_string(),
            })
            .collect();

        write!(f, "{}", segments.join(" | "))
    }
}

impl fmt::Debug for VersionSet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "VersionSet({self})")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn test_set_algebra() {
        let v1 = Version::from_str("1.0.0").unwrap();
        let v2 = Version::from_str("2.0.0").unwrap();
        let v3 = Version::from_str("3.0.0").unwrap();

        let s1 = VersionSet::singleton(&v1);
        let s2 = VersionSet::singleton(&v2);

        assert!(s1.contains(&v1));
        assert!(!s1.contains(&v2));
        assert!(s1.is_disjoint(&s2));

        let union = s1.union(&s2);
        assert!(union.contains(&v1));
        assert!(union.contains(&v2));
        assert!(!union.contains(&v3));

        let comp = s1.complement();
        assert!(!comp.contains(&v1));
        assert!(comp.contains(&v2));
        assert!(comp.contains(&v3));
    }

    #[test]
    fn test_from_specifiers() {
        let specs = VersionSpecifiers::from_str(">=1.5.0, <2.0.0").unwrap();
        let set = VersionSet::from_specifiers(&specs);

        let v_low = Version::from_str("1.4.9").unwrap();
        let v_mid = Version::from_str("1.8.0").unwrap();
        let v_high = Version::from_str("2.0.0").unwrap();

        assert!(!set.contains(&v_low));
        assert!(set.contains(&v_mid));
        assert!(!set.contains(&v_high));
    }
}
