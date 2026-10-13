//! Starting a browser from the command line: shared by the terminal and window apps.

use crate::{Browser, Capabilities};

use std::path::PathBuf;
use std::rc::Rc;

use engine::{DefaultLoader, Registry};
use profile::{Paths, Profile};

/// What the command line asked for.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Options {
    /// `--profile NAME`: a profile in the config directory (default `default`).
    pub profile: Option<String>,
    /// `--profile-file PATH`: a profile file (.toml, .json, .yaml or .yml) anywhere.
    pub profile_file: Option<PathBuf>,
    /// `--private`: a throwaway profile that is never saved.
    pub private: bool,
    /// Addresses and files to open as tabs; empty means the saved tabs or the home page.
    pub urls: Vec<String>,
    /// `--follower`: set on windows opened from another window (see `Browser::set_follower`).
    pub follower: bool,
    pub help: bool,
}

pub const OPTIONS_HELP: &str = "  --profile NAME       use the profile NAME (default: default)\n\
  --profile-file PATH  use a profile file (.toml, .json, .yaml, .yml)\n\
  --private            use a throwaway profile that is never saved";

impl Options {
    /// Parses arguments (without the program name).
    pub fn parse<I: IntoIterator<Item = String>>(args: I) -> Result<Options, String> {
        let mut o = Options::default();
        let mut args = args.into_iter();
        let mut only_urls = false;
        while let Some(a) = args.next() {
            if only_urls {
                o.urls.push(a);
                continue;
            }
            match a.as_str() {
                "--" => only_urls = true,
                "-h" | "--help" => o.help = true,
                "--private" => o.private = true,
                "--follower" => o.follower = true,
                "--profile" => {
                    o.profile = Some(args.next().ok_or("--profile needs a name")?);
                }
                "--profile-file" => {
                    o.profile_file = Some(PathBuf::from(
                        args.next().ok_or("--profile-file needs a path")?,
                    ));
                }
                s if s.starts_with("--") => return Err(format!("unknown option {}", s)),
                _ => o.urls.push(a),
            }
        }
        if o.private && (o.profile.is_some() || o.profile_file.is_some()) {
            return Err(
                "--private cannot be combined with --profile or --profile-file".to_string(),
            );
        }
        if o.profile.is_some() && o.profile_file.is_some() {
            return Err("use either --profile or --profile-file, not both".to_string());
        }
        Ok(o)
    }

    /// The arguments that select the same profile again (for opening another window).
    pub fn profile_args(&self) -> Vec<String> {
        let mut a = Vec::new();
        if self.private {
            a.push("--private".to_string());
        }
        if let Some(p) = &self.profile {
            a.push("--profile".to_string());
            a.push(p.clone());
        }
        if let Some(f) = &self.profile_file {
            a.push("--profile-file".to_string());
            a.push(f.display().to_string());
        }
        a
    }
}

/// Opens the profile and builds a browser with its first tabs. Problems that do not stop
/// the browser (a bad value in the profile file, no home directory) come back as warnings.
pub fn open(
    opts: &Options,
    paths: &Paths,
    cols: usize,
    rows: usize,
    caps: Capabilities,
) -> Result<(Browser, Vec<String>), String> {
    let mut warnings = Vec::new();
    let name = opts
        .profile
        .clone()
        .unwrap_or_else(|| "default".to_string());

    let mut profile = if opts.private {
        Profile::new("private")?
    } else if let Some(file) = &opts.profile_file {
        let stem = file
            .file_stem()
            .and_then(|s| s.to_str())
            .filter(|s| profile::check_name(s).is_ok())
            .unwrap_or("custom")
            .to_string();
        let (p, w) = Profile::load_or_new(file, &stem)?;
        warnings.extend(w);
        p
    } else {
        match paths.profile_file(&name) {
            Ok(file) => {
                let (p, w) = Profile::load_or_new(&file, &name)?;
                warnings.extend(w);
                p
            }
            Err(e) if opts.profile.is_none() => {
                warnings.push(format!("{}; this session is not saved", e));
                Profile::new(&name)?
            }
            Err(e) => return Err(e),
        }
    };
    if opts.private {
        // Plugins of a private session must not outlive it.
        profile.storage = profile::Storage::Tmp;
    }

    let registry = match profile.open_registry(paths) {
        Ok(r) => r,
        Err(e) => {
            warnings.push(format!("plugins are kept in memory only: {}", e));
            Registry::in_memory()
        }
    };
    let mut b = Browser::new(
        profile,
        registry,
        Rc::new(DefaultLoader::default()),
        cols,
        rows,
    );
    b.set_capabilities(caps);
    b.set_follower(opts.follower);
    b.start(&opts.urls);
    Ok((b, warnings))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(s: &[&str]) -> Result<Options, String> {
        Options::parse(s.iter().map(|s| s.to_string()))
    }

    #[test]
    fn parses_flags_and_urls() {
        let o = parse(&["--profile", "work", "a.rs", "https://x.y"]).unwrap();
        assert_eq!(o.profile.as_deref(), Some("work"));
        assert_eq!(o.urls, vec!["a.rs".to_string(), "https://x.y".to_string()]);
        assert!(parse(&["--private"]).unwrap().private);
        assert!(parse(&["-h"]).unwrap().help);
        let o = parse(&["--", "--weird"]).unwrap();
        assert_eq!(o.urls, vec!["--weird".to_string()]);
    }

    #[test]
    fn profile_args_select_the_same_profile() {
        let o = parse(&["--profile", "work", "x.rs"]).unwrap();
        assert_eq!(
            o.profile_args(),
            vec!["--profile".to_string(), "work".to_string()]
        );
        assert_eq!(
            parse(&["--private"]).unwrap().profile_args(),
            vec!["--private".to_string()]
        );
        assert!(parse(&[]).unwrap().profile_args().is_empty());
        let o = parse(&["--profile-file", "p.json"]).unwrap();
        assert_eq!(
            parse(
                &o.profile_args()
                    .iter()
                    .map(|s| s.as_str())
                    .collect::<Vec<_>>()
            )
            .unwrap(),
            Options { urls: vec![], ..o }
        );
    }

    #[test]
    fn followers_do_not_overwrite_the_session() {
        let mut dir = std::env::temp_dir();
        dir.push(format!("brewster-follower-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let paths = Paths {
            home: Some(dir.clone()),
            tmp: dir.join("tmp"),
            config: Some(dir.join("cfg")),
        };
        let first = Options {
            urls: vec!["about:blank".to_string()],
            ..Options::default()
        };
        let (mut a, _) = open(&first, &paths, 40, 10, Capabilities::default()).unwrap();
        a.save_profile().unwrap();
        let second = Options {
            urls: vec!["about:newtab".to_string()],
            follower: true,
            ..Options::default()
        };
        let (mut b, _) = open(&second, &paths, 40, 10, Capabilities::default()).unwrap();
        b.on_key(crate::KeyPress::plain(crate::Key::Char('q')));
        assert!(b.should_quit());
        let (c, _) = open(&Options::default(), &paths, 40, 10, Capabilities::default()).unwrap();
        assert_eq!(c.tab_url(0).as_deref(), Some("about:blank"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn rejects_bad_combinations() {
        assert!(parse(&["--profile"]).is_err());
        assert!(parse(&["--nope"]).is_err());
        assert!(parse(&["--private", "--profile", "a"]).is_err());
        assert!(parse(&["--profile", "a", "--profile-file", "b.toml"]).is_err());
    }

    #[test]
    fn opens_a_saved_profile_and_its_tabs() {
        let mut dir = std::env::temp_dir();
        dir.push(format!("brewster-launch-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let paths = Paths {
            home: Some(dir.clone()),
            tmp: dir.join("tmp"),
            config: Some(dir.join("cfg")),
        };
        let opts = Options {
            urls: vec!["about:blank".to_string()],
            ..Options::default()
        };
        let (mut b, warnings) = open(&opts, &paths, 40, 10, Capabilities::default()).unwrap();
        assert!(warnings.is_empty(), "{:?}", warnings);
        assert_eq!(b.tab_count(), 1);
        b.save_profile().unwrap();
        assert!(dir.join("cfg/profiles/default.toml").exists());
        // Reopening restores the tab.
        let (b2, _) = open(&Options::default(), &paths, 40, 10, Capabilities::default()).unwrap();
        assert_eq!(b2.tab_url(0).as_deref(), Some("about:blank"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn no_home_directory_gives_a_warning_not_an_error() {
        let paths = Paths {
            home: None,
            tmp: std::env::temp_dir().join("brewster-launch-nohome"),
            config: None,
        };
        let (b, warnings) =
            open(&Options::default(), &paths, 40, 10, Capabilities::default()).unwrap();
        assert!(
            warnings.iter().any(|w| w.contains("not saved")),
            "{:?}",
            warnings
        );
        assert!(b.tab_count() >= 1);
        assert!(
            open(
                &Options {
                    profile: Some("x".into()),
                    ..Options::default()
                },
                &paths,
                40,
                10,
                Capabilities::default()
            )
            .is_err()
        );
    }
}
