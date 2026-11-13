use std::net::IpAddr;

use url::Url;

use crate::config::Config;
use crate::error::Error;

const LOOPBACK_NAMES: [&str; 3] = ["localhost", "127.0.0.1", "[::1]"];
const APP: &str = "translation-inference";

/// Where the app is served from, derived once from the config.
#[derive(Debug, Clone)]
pub struct Deployment {
    pub origin: String,
    /// https in front: HSTS and a `__Host-` cookie with `Secure`.
    pub secure: bool,
    /// Host of `public_url`, with the port when it is not the scheme's default.
    pub public_host: String,
    /// No `public_url` and a loopback bind: any loopback name with the bind port is answered.
    pub loopback_dev: bool,
    pub port: u16,
    /// `allow_plain_http` with a non-loopback `public_url`.
    pub plain_http: bool,
}

fn host_with_port(url: &Url) -> String {
    let host = url.host_str().unwrap_or_default();
    match url.port() {
        Some(p) => format!("{host}:{p}"),
        None => host.to_owned(),
    }
}

fn is_loopback(host: &str) -> bool {
    host == "localhost"
        || host.trim_start_matches('[').trim_end_matches(']').parse::<IpAddr>().is_ok_and(|ip| ip.is_loopback())
}

impl Deployment {
    pub fn from_config(cfg: &Config) -> Result<Self, Error> {
        let Some(url) = &cfg.public_url else {
            if !cfg.bind.ip().is_loopback() {
                return Err(Error::invalid("Set public_url when binding to a non-loopback address."));
            }
            let port = cfg.bind.port();
            return Ok(Self {
                origin: format!("http://localhost:{port}"),
                secure: false,
                public_host: format!("localhost:{port}"),
                loopback_dev: true,
                port,
                plain_http: false,
            });
        };
        let host = url.host_str().unwrap_or_default();
        let loopback = is_loopback(host);
        let plain_http = match url.scheme() {
            "https" => false,
            "http" if loopback => false,
            "http" if cfg.allow_plain_http => true,
            _ => {
                return Err(Error::invalid(
                    "public_url must be https unless it is localhost or allow_plain_http is on.",
                ));
            }
        };
        Ok(Self {
            origin: url.origin().ascii_serialization(),
            secure: url.scheme() == "https",
            public_host: host_with_port(url),
            loopback_dev: false,
            port: url.port_or_known_default().unwrap_or(443),
            plain_http,
        })
    }

    pub fn host_allowed(&self, host: &str) -> bool {
        if self.loopback_dev {
            return LOOPBACK_NAMES
                .iter()
                .any(|n| host == format!("{n}:{}", self.port) || (self.port == 80 && host == *n));
        }
        host == self.public_host
    }

    /// Whether a request may come from `origin`.
    pub fn origin_allowed(&self, origin: &str) -> bool {
        if self.loopback_dev {
            // Browsers leave the scheme's default port out of an Origin, as out of a Host.
            return LOOPBACK_NAMES.iter().any(|n| {
                origin == format!("http://{n}:{}", self.port) || (self.port == 80 && origin == format!("http://{n}"))
            });
        }
        origin == self.origin
    }

    /// Browsers keep `Secure` cookies to secure contexts, which loopback is and a plain LAN
    /// address is not; only `Secure` cookies may carry the `__Host-` prefix.
    pub fn cookie_name(&self) -> String {
        if self.plain_http { format!("{APP}-session") } else { format!("__Host-{APP}-session") }
    }

    pub fn cookie_secure(&self) -> bool {
        !self.plain_http
    }
}
