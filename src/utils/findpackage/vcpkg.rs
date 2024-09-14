use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use crate::Url;

use std::sync::LazyLock;

use crate::utils::{CMakePackage, FileType};

use super::{get_version, CMAKECONFIG, CMAKECONFIGVERSION, CMAKEREGEX};

#[inline]
pub fn did_vcpkg_project(path: &Path) -> bool {
    path.is_dir() && path.join("vcpkg.json").is_file()
}

pub static VCPKG_PREFIX: LazyLock<Arc<Mutex<Vec<&str>>>> =
    LazyLock::new(|| Arc::new(Mutex::new([].to_vec())));

pub static VCPKG_LIBS: LazyLock<Arc<Mutex<Vec<&str>>>> =
    LazyLock::new(|| Arc::new(Mutex::new([].to_vec())));

#[cfg(not(windows))]
fn safe_canonicalize(path: &Path) -> std::io::Result<PathBuf> {
    path.canonicalize()
}

#[cfg(windows)]
fn safe_canonicalize(path: &Path) -> std::io::Result<PathBuf> {
    use path_absolutize::Absolutize;
    Ok(path.absolutize()?.into_owned())
}

fn get_available_libs() -> Vec<PathBuf> {
    let mut ava: Vec<PathBuf> = vec![];
    let vcpkg_prefix = VCPKG_PREFIX.lock().unwrap();
    let vcpkg_libs = VCPKG_LIBS.lock().unwrap();
    for prefix in vcpkg_prefix.iter() {
        for lib in vcpkg_libs.iter() {
            let p = Path::new(prefix).join(lib);
            if p.exists() {
                ava.push(p);
            }
        }
    }
    ava
}

fn get_cmake_message() -> HashMap<String, CMakePackage> {
    let mut packages: HashMap<String, CMakePackage> = HashMap::new();
    let vcpkg_prefix = VCPKG_PREFIX.lock().unwrap();
    for lib in vcpkg_prefix.iter() {
        let Ok(paths) = glob::glob(&format!("{lib}/share/*/cmake/")) else {
            continue;
        };
        for path in paths.flatten() {
            let Ok(files) = glob::glob(&format!("{}/*.cmake", path.to_string_lossy())) else {
                continue;
            };
            let mut tojump: Vec<PathBuf> = vec![];
            let mut version: Option<String> = None;
            let mut ispackage = false;
            for f in files.flatten() {
                tojump.push(safe_canonicalize(&f).unwrap());
                if CMAKECONFIG.is_match(f.to_str().unwrap()) {
                    ispackage = true;
                }
                if CMAKECONFIGVERSION.is_match(f.to_str().unwrap()) {
                    if let Ok(context) = fs::read_to_string(&f) {
                        version = get_version(&context);
                    }
                }
            }
            if ispackage {
                let filepath = Url::from_file_path(&path).unwrap();
                let packagename = path
                    .parent()
                    .unwrap()
                    .file_name()
                    .unwrap()
                    .to_str()
                    .unwrap();
                packages
                    .entry(packagename.to_string())
                    .or_insert_with(|| CMakePackage {
                        name: packagename.to_string(),
                        filetype: FileType::Dir,
                        filepath,
                        version,
                        tojump,
                        from: "Vcpkg".to_string(),
                    });
            }
        }
    }
    drop(vcpkg_prefix);
    for lib in get_available_libs() {
        let Ok(paths) = std::fs::read_dir(lib) else {
            continue;
        };
        for path in paths.flatten() {
            let mut version: Option<String> = None;
            let mut tojump: Vec<PathBuf> = vec![];
            let pathname = path.file_name().to_str().unwrap().to_string();
            let packagepath = Url::from_file_path(path.path()).unwrap();
            let (packagetype, packagename) = {
                if path.metadata().is_ok_and(|data| data.is_dir()) {
                    let Ok(paths) = std::fs::read_dir(path.path()) else {
                        continue;
                    };
                    for path in paths.flatten() {
                        let filepath = safe_canonicalize(&path.path()).unwrap();
                        if path.metadata().unwrap().is_file() {
                            let filename = path.file_name().to_str().unwrap().to_string();
                            if CMAKEREGEX.is_match(&filename) {
                                tojump.push(filepath.clone());
                                if CMAKECONFIGVERSION.is_match(&filename) {
                                    if let Ok(context) = fs::read_to_string(&filepath) {
                                        version = get_version(&context);
                                    }
                                }
                            }
                        }
                    }
                    (FileType::Dir, pathname)
                } else {
                    let filepath = safe_canonicalize(&path.path()).unwrap();
                    tojump.push(filepath);
                    let pathname = pathname.split('.').collect::<Vec<&str>>()[0].to_string();
                    (FileType::File, pathname)
                }
            };
            packages
                .entry(packagename.clone())
                .or_insert_with(|| CMakePackage {
                    name: packagename,
                    filetype: packagetype,
                    filepath: packagepath,
                    version,
                    tojump,
                    from: "Vcpkg".to_string(),
                });
        }
    }
    packages
}

pub fn make_vcpkg_package_search_path(search_path: &Path) -> std::io::Result<Vec<String>> {
    const LIB_PATHS: [&str; 6] = [
        "x64-linux",
        "x86-linux",
        "x64-windows",
        "x86-windows",
        "x64-osx",
        "arm64-osx",
    ];

    let mut paths: Vec<String> = Vec::new();

    // check search path is ok
    for item in LIB_PATHS {
        if search_path.join(item).is_dir() {
            let path = Path::new(item).join("share");
            paths.push(path.to_str().unwrap().to_string());
        }
    }

    Ok(paths)
}

pub static VCPKG_CMAKE_PACKAGES: LazyLock<Vec<CMakePackage>> =
    LazyLock::new(|| get_cmake_message().into_values().collect());
pub static VCPKG_CMAKE_PACKAGES_WITHKEY: LazyLock<HashMap<String, CMakePackage>> =
    LazyLock::new(get_cmake_message);

#[test]
fn test_vcpkgpackage_search() {
    use std::fs;
    use std::fs::File;
    use std::io::Write;
    use tempfile::tempdir;
    let dir = tempdir().unwrap();

    let vcpkg_path = dir.path().join("vcpkg.json");
    File::create(vcpkg_path).unwrap();

    assert!(did_vcpkg_project(dir.path()));

    let prefix_dir = safe_canonicalize(dir.path())
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    let mut prefix = VCPKG_PREFIX.lock().unwrap();

    prefix.push(Box::leak(prefix_dir.into_boxed_str()));
    drop(prefix);

    let mut libs = VCPKG_LIBS.lock().unwrap();
    libs.push("x64-linux");
    drop(libs);

    let share_path = dir.path().join("share");

    let ecm_dir = share_path.join("ECM").join("cmake");
    fs::create_dir_all(&ecm_dir).unwrap();
    let ecm_config_cmake = ecm_dir.join("ECMConfig.cmake");
    File::create(&ecm_config_cmake).unwrap();
    let ecm_config_version_cmake = ecm_dir.join("ECMConfigVersion.cmake");
    let mut ecm_config_version_file = File::create(&ecm_config_version_cmake).unwrap();
    writeln!(ecm_config_version_file, r#"set(PACKAGE_VERSION "6.5.0")"#).unwrap();

    let target = HashMap::from_iter([(
        "ECM".to_string(),
        CMakePackage {
            name: "ECM".to_string(),
            filetype: FileType::Dir,
            filepath: Url::from_file_path(ecm_dir).unwrap(),
            version: Some("6.5.0".to_string()),
            tojump: vec![ecm_config_cmake, ecm_config_version_cmake],
            from: "Vcpkg".to_string(),
        },
    )]);
    assert_eq!(get_cmake_message(), target);
}
