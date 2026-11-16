//! Offers the latest GitHub release of the plugin. The SDK cannot install packages, so the offer
//! downloads the `.oiplugin` for Settings -> Plugins -> Install Plugin from Disk.
use openidea_plugin::{Error, HttpRequest, NotificationAction, NotificationLevel, host};
use regex::Regex;
use std::sync::OnceLock;

pub const COMMAND: &str = "update";
/// GitHub's `releases/latest` skips drafts and pre-releases.
const LATEST_RELEASE_API: &str = "https://api.github.com/repos/pxc1984/openidea-riscv-plugin/releases/latest";
pub const DOWNLOAD_URL: &str =
    "https://github.com/pxc1984/openidea-riscv-plugin/releases/latest/download/openidea.riscv.oiplugin";
const LAST_CHECK: &str = "update-checked-at";
/// Хост выгружает плагин после 5 минут простоя и активирует заново, а признака нового запуска
/// IDE в API нет: без паузы предложение всплывало бы при каждой активации.
pub const CHECK_INTERVAL_SECS: u64 = 12 * 60 * 60;

/// Offers an update when GitHub's latest release is newer than `installed`. At most once per
/// [`CHECK_INTERVAL_SECS`]; network failures and a refused `net:api.github.com` stay silent.
pub fn check(installed: &str, now_secs: u64) -> Result<(), Error> {
    let last = host::storage::get(LAST_CHECK)?.and_then(|s| s.parse::<u64>().ok());
    if last.is_some_and(|last| now_secs.saturating_sub(last) < CHECK_INTERVAL_SECS) {
        return Ok(());
    }
    host::storage::set(LAST_CHECK, &now_secs.to_string())?;
    let mut request = HttpRequest::get(LATEST_RELEASE_API);
    request.headers = vec![
        ("Accept".into(), "application/vnd.github+json".into()),
        // GitHub API отклоняет запросы без User-Agent.
        ("User-Agent".into(), "openidea-riscv-plugin".into()),
    ];
    request.timeout_ms = Some(3000);
    let response = host::fetch(&request)?;
    if response.status != 200 {
        return Ok(());
    }
    let body = String::from_utf8_lossy(&response.body);
    let Some(tag) = latest_tag(&body) else {
        return Ok(());
    };
    if !is_newer(tag, installed) {
        return Ok(());
    }
    host::notify_with_actions(
        NotificationLevel::Info,
        "RISC-V",
        &format!("RISC-V plugin {tag} is available (installed {installed}). Download it and install it from disk."),
        &[NotificationAction::new(format!("Download {tag}"), COMMAND)],
    )
}

fn latest_tag(body: &str) -> Option<&str> {
    static TAG: OnceLock<Regex> = OnceLock::new();
    let tag = TAG.get_or_init(|| Regex::new(r#""tag_name"\s*:\s*"([^"\\]+)""#).unwrap());
    Some(tag.captures(body)?.get(1)?.as_str())
}

fn version(text: &str) -> Option<(u64, u64, u64)> {
    let mut parts = text.trim_start_matches('v').split('.').map(|p| p.parse().ok());
    let parsed = (parts.next()??, parts.next()??, parts.next()??);
    parts.next().is_none().then_some(parsed)
}

/// Whether release tag `tag` (`v1.2.3`) is a later version than `installed` (`1.2.3`).
pub fn is_newer(tag: &str, installed: &str) -> bool {
    version(tag).zip(version(installed)).is_some_and(|(tag, installed)| tag > installed)
}
