use std::{
    collections::HashMap,
    fs::{create_dir_all, read_dir, remove_file},
    path::Path,
};

use chrono::{Local, NaiveDateTime};
use directories::ProjectDirs;
use fern::Dispatch;
use log::LevelFilter;

const CHRONO_FORMAT: &str = "%Y-%m-%d_%H-%M-%S";
const MAX_LOG_FILES: u8 = 5;

pub struct LoggingBuilder {
    app_name: String,

    global_level: LevelFilter,

    qualifier: String,
    organization: String,

    level_for: HashMap<String, LevelFilter>,
}

impl LoggingBuilder {
    pub fn new() -> Self {
        Self {
            app_name: "".to_string(),

            global_level: LevelFilter::Debug,

            qualifier: "".to_string(),
            organization: "".to_string(),

            level_for: HashMap::new(),
        }
    }

    pub fn app_name(mut self, app_name: impl ToString) -> Self {
        self.app_name = app_name.to_string();

        self
    }

    pub fn global_level(mut self, level: LevelFilter) -> Self {
        self.global_level = level;

        self
    }

    pub fn qualifier(mut self, qualifier: impl ToString) -> Self {
        self.qualifier = qualifier.to_string();

        self
    }

    pub fn organization(mut self, organization: impl ToString) -> Self {
        self.organization = organization.to_string();

        self
    }

    pub fn level_for(mut self, module: impl ToString, level: LevelFilter) -> Self {
        self.level_for.insert(module.to_string(), level);

        self
    }

    pub fn finish(self) -> anyhow::Result<()> {
        if self.app_name.is_empty() || self.qualifier.is_empty() || self.organization.is_empty() {
            anyhow::bail!("Missing required fields")
        }

        let term = Dispatch::new()
            .format(|out, message, record| {
                out.finish(format_args!(
                    "[{}] {} - {}",
                    record.level(),
                    record.target(),
                    message
                ))
            })
            .level(LevelFilter::Debug)
            .chain(std::io::stdout());

        let project_dir = if let Some(d) =
            ProjectDirs::from(&self.qualifier, &self.organization, &self.app_name)
        {
            d
        } else {
            anyhow::bail!("Unable to get project directories");
        };
        let mut log_dir = project_dir.cache_dir().to_path_buf();
        log_dir.push("logs");

        rotate_logs(&log_dir)?;

        let time = Local::now();

        let mut log_file_path = log_dir;
        log_file_path.push(format!("{}.log", time.format(CHRONO_FORMAT)));

        let file = Dispatch::new()
            .format(|out, message, record| {
                out.finish(format_args!(
                    "[{}] {} {} - {}",
                    record.level(),
                    Local::now().naive_local().format(CHRONO_FORMAT),
                    record.target(),
                    message
                ))
            })
            .level(LevelFilter::Debug)
            .chain(fern::log_file(log_file_path)?);

        let mut root = Dispatch::new().level(self.global_level);
        for (mod_name, level) in self.level_for.iter() {
            root = root.level_for(mod_name.clone(), level.clone());
        }

        root.chain(term).chain(file).apply()?;

        Ok(())
    }
}

/// Rotates all logs found in the `log_dir`.
fn rotate_logs<P: AsRef<Path>>(log_dir: P) -> anyhow::Result<()> {
    let mut logs = get_all_logs(log_dir)?;

    while logs.len() >= MAX_LOG_FILES.into() {
        let path = logs.pop().unwrap();

        remove_file(path)?;
    }

    Ok(())
}

/// Gets all log files from the `log_dir` sorted by date.
///
/// **WARNING**: Any log file that cannot be parsed is deleted.
fn get_all_logs<P: AsRef<Path>>(log_dir: P) -> anyhow::Result<Vec<String>> {
    let log_dir = log_dir.as_ref();

    if !log_dir.exists() {
        create_dir_all(&log_dir)?;
    }

    let mut log_files = vec![];

    let paths = read_dir(&log_dir)?;
    for path in paths {
        let path = path?.path();
        let file_path = path.display().to_string();
        let file_name = if let Some(n) = path.file_stem() {
            n.to_str().unwrap_or_default()
        } else {
            continue;
        };

        let time = if let Ok(v) = NaiveDateTime::parse_from_str(file_name, CHRONO_FORMAT) {
            v
        } else {
            std::fs::remove_file(path)?;
            continue;
        };

        log_files.push((file_path, time));
    }

    sort_log_files(&mut log_files);

    Ok(log_files.iter().map(|(path, _)| path.to_string()).collect())
}

/// Intentionally split out to make it easier to test.
#[inline]
fn sort_log_files(logs: &mut Vec<(String, NaiveDateTime)>) {
    logs.sort_by(|(_, a), (_, b)| b.cmp(a));
}

#[cfg(test)]
mod tests {
    use chrono::{Duration, Local, NaiveDateTime};

    use crate::CHRONO_FORMAT;

    #[test]
    fn sort_log_files() {
        let mut test_logs = vec![];

        for file in vec![
            Local::now()
                .naive_local()
                .checked_add_signed(Duration::seconds(60))
                .unwrap()
                .format(CHRONO_FORMAT),
            Local::now()
                .naive_local()
                .checked_add_signed(Duration::seconds(160))
                .unwrap()
                .format(CHRONO_FORMAT),
            Local::now()
                .naive_local()
                .checked_add_signed(Duration::seconds(260))
                .unwrap()
                .format(CHRONO_FORMAT),
        ] {
            test_logs.push(file.to_string());
        }

        let mut logs = vec![];

        for file in test_logs {
            let time = if let Ok(v) = NaiveDateTime::parse_from_str(file.as_str(), CHRONO_FORMAT) {
                v
            } else {
                continue;
            };

            logs.push((file, time));
        }

        crate::sort_log_files(&mut logs);

        assert!(logs[0].1 > logs[1].1);
        assert!(logs[1].1 > logs[2].1);
    }
}
