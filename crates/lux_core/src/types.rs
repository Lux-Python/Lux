pub mod error;
pub mod marker;
pub mod package_name;
pub mod requirement;
pub mod version;
pub mod version_specifier;

pub use error::TypeError;
pub use marker::EnvironmentMarker;
pub use package_name::{normalize_name, PackageName};
pub use requirement::Requirement;
pub use version::Version;
pub use version_specifier::{VersionSpecifier, VersionSpecifiers};

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn test_pep440_full_matrix() {
        let cases = [
            ("1.0.0", false, false, false),
            ("2.0.0a1", true, false, false),
            ("2.0.0b2", true, false, false),
            ("2.0.0rc1", true, false, false),
            ("1.2.3.post1", false, true, false),
            ("1.2.3.dev4", false, false, true),
        ];

        for (v_str, is_pre, is_post, is_dev) in cases {
            let v = Version::from_str(v_str).expect("should parse");
            assert_eq!(v.is_prerelease(), is_pre, "prerelease mismatch for {v_str}");
            assert_eq!(v.is_postrelease(), is_post, "postrelease mismatch for {v_str}");
            assert_eq!(v.is_dev(), is_dev, "dev mismatch for {v_str}");
        }
    }

    #[test]
    fn test_pep440_specifier_matching() {
        let specs = VersionSpecifiers::from_str(">=1.21.0, <2.0.0, !=1.24.0").unwrap();

        let v1 = Version::from_str("1.21.0").unwrap();
        let v2 = Version::from_str("1.23.5").unwrap();
        let v_excluded = Version::from_str("1.24.0").unwrap();
        let v_out_of_bounds = Version::from_str("2.0.0").unwrap();

        assert!(specs.contains(&v1));
        assert!(specs.contains(&v2));
        assert!(!specs.contains(&v_excluded));
        assert!(!specs.contains(&v_out_of_bounds));
    }

    #[test]
    fn test_pep508_requirement_with_markers() {
        let req_str = "requests[security] >= 2.28.0; python_version >= '3.8' and os_name == 'posix'";
        let req = Requirement::from_str(req_str).expect("should parse requirement");

        assert_eq!(req.name().as_str(), "requests");
        assert_eq!(req.extras().len(), 1);
        assert_eq!(req.extras()[0].as_str(), "security");
        assert!(req.marker().is_some());

        let ver_ok = Version::from_str("2.28.1").unwrap();
        let ver_old = Version::from_str("2.25.0").unwrap();
        assert!(req.is_satisfied_by(&ver_ok));
        assert!(!req.is_satisfied_by(&ver_old));
    }

    #[test]
    fn test_zero_copy_interning_smol_str() {
        let pkg = PackageName::new("numpy").unwrap();
        // numpy is 5 bytes, fits directly inside SmolStr inline buffer (<= 23 bytes)
        assert_eq!(pkg.as_str(), "numpy");
        assert_eq!(pkg.len(), 5);
    }
}
