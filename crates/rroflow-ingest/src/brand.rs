//! Deployment-wide presentation. No credentials, scripts or data destinations.
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct BrandConfig {
    pub name: String,
    pub wordmark: String,
    pub mark: String,
    pub tagline: String,
    pub intro_title: String,
    pub intro_emphasis: String,
    pub intro_description: String,
    pub documentation_url: String,
    pub links: Vec<BrandLink>,
    pub accent: String,
    pub highlight: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BrandLink {
    pub label: String,
    pub url: String,
}

impl Default for BrandConfig {
    fn default() -> Self {
        Self {
            name: "RROFLOW".into(), wordmark: "rroflow.".into(), mark: "r".into(),
            tagline: "Reason Ready Objects and flow".into(),
            intro_title: "Your AI does a lot.".into(),
            intro_emphasis: "See the whole story.".into(),
            intro_description: "What happened? Where did it get stuck? What was worth keeping? Turn your agents’ activity into a clear picture you can actually use.".into(),
            documentation_url: "https://github.com/rrflow/rroflow-development/blob/main/docs/plan/sdk-install-and-linking.md".into(),
            links: vec![
                BrandLink { label: "Source & documentation ↗".into(), url: "https://github.com/rrflow/rroflow-development".into() },
                BrandLink { label: "RRFlow reasoning engine ↗".into(), url: "https://rrflow.ai".into() },
                BrandLink { label: "Shippin.cloud ↗".into(), url: "https://shippin.cloud".into() },
            ],
            accent: "#36634e".into(), highlight: "#d6f27c".into(),
        }
    }
}

fn text(value: &str, max: usize, field: &str) -> Result<()> {
    ensure!(
        !value.trim().is_empty()
            && value.chars().count() <= max
            && !value.chars().any(char::is_control),
        "brand.{field} must contain 1..={max} printable characters"
    );
    Ok(())
}

fn url(value: &str) -> Result<()> {
    ensure!(
        value.len() <= 2048
            && !value
                .chars()
                .any(|c| c.is_whitespace() || c.is_control() || c == '\\'),
        "invalid brand link"
    );
    let parsed = reqwest::Url::parse(value)
        .map_err(|_| anyhow::anyhow!("brand links require an absolute HTTPS URL"))?;
    ensure!(
        parsed.scheme() == "https"
            && parsed.host_str().is_some()
            && parsed.username().is_empty()
            && parsed.password().is_none(),
        "brand links require HTTPS without credentials"
    );
    Ok(())
}

fn luminance(color: &str) -> Result<f64> {
    ensure!(
        color.len() == 7
            && color.starts_with('#')
            && color.as_bytes()[1..].iter().all(u8::is_ascii_hexdigit),
        "brand colors must use #RRGGBB"
    );
    let channel = |i| {
        let v = u8::from_str_radix(&color[i..i + 2], 16).unwrap() as f64 / 255.0;
        if v <= 0.04045 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    Ok(0.2126 * channel(1) + 0.7152 * channel(3) + 0.0722 * channel(5))
}

fn contrast(a: f64, b: f64) -> f64 {
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

impl BrandConfig {
    pub(crate) fn validate(&self) -> Result<()> {
        for (field, value, max) in [
            ("name", &self.name, 80),
            ("wordmark", &self.wordmark, 24),
            ("mark", &self.mark, 2),
            ("tagline", &self.tagline, 160),
            ("intro_title", &self.intro_title, 120),
            ("intro_emphasis", &self.intro_emphasis, 120),
            ("intro_description", &self.intro_description, 500),
        ] {
            text(value, max, field)?;
        }
        url(&self.documentation_url)?;
        ensure!(
            self.links.len() <= 6,
            "brand.links allows at most six links"
        );
        for link in &self.links {
            text(&link.label, 60, "links.label")?;
            url(&link.url)?;
        }
        let accent = luminance(&self.accent)?;
        // Accent text appears on these existing light surfaces. Keep semantic
        // error/warning colors independent from operator branding.
        for background in ["#ffffff", "#faf9f6", "#faf9f3", "#edf4e5", "#edf1e6"] {
            ensure!(
                contrast(accent, luminance(background)?) >= 4.5,
                "brand.accent needs at least 4.5:1 contrast on light surfaces; choose a darker color"
            );
        }
        let highlight = luminance(&self.highlight)?;
        for ink in [
            "#293c35", "#2b4034", "#283d35", "#324735", "#393241", "#443b2f",
        ] {
            ensure!(
                contrast(highlight, luminance(ink)?) >= 4.5,
                "brand.highlight needs at least 4.5:1 contrast with dark ink; choose a lighter color"
            );
        }
        Ok(())
    }
}

fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

/// Only replace explicitly authored slots. User values are never parsed as templates.
fn slot(html: &mut String, start: &str, end: &str, value: &str) -> Result<()> {
    let mut offset = 0;
    let mut count = 0;
    while let Some(index) = html[offset..].find(start) {
        let begin = offset + index + start.len();
        let finish = begin
            + html[begin..]
                .find(end)
                .ok_or_else(|| anyhow::anyhow!("invalid authored brand slot"))?;
        html.replace_range(begin..finish, value);
        offset = begin + value.len();
        count += 1;
    }
    ensure!(count > 0, "missing authored brand slot {start}");
    Ok(())
}

pub(crate) struct RenderedBrand {
    pub public: String,
    pub dashboard: String,
    pub css: String,
}

impl RenderedBrand {
    pub fn new(config: &BrandConfig) -> Result<Self> {
        config.validate()?;
        let mut public = include_str!("public.html").to_owned();
        let mut dashboard = include_str!("dashboard.html").to_owned();
        for html in [&mut public, &mut dashboard] {
            for (key, value) in [
                ("name", &config.name),
                ("wordmark", &config.wordmark),
                ("mark", &config.mark),
                ("tagline", &config.tagline),
            ] {
                slot(html, &format!("data-brand=\"{key}\">"), "<", &escape(value))?;
            }
        }
        for (key, value) in [
            ("intro_title", &config.intro_title),
            ("intro_emphasis", &config.intro_emphasis),
            ("intro_description", &config.intro_description),
        ] {
            slot(
                &mut public,
                &format!("data-brand=\"{key}\">"),
                "<",
                &escape(value),
            )?;
        }
        slot(
            &mut public,
            "data-brand=\"public_title\">",
            "<",
            &escape(&format!("{} · A clearer picture of your AI", config.name)),
        )?;
        slot(
            &mut dashboard,
            "data-brand=\"member_title\">",
            "<",
            &escape(&format!("{} · Your AI activity", config.name)),
        )?;
        slot(
            &mut public,
            "data-brand-description content=\"",
            "\"",
            &escape(&config.intro_description),
        )?;
        slot(
            &mut dashboard,
            "data-brand-docs href=\"",
            "\"",
            &escape(&config.documentation_url),
        )?;
        let links = config
            .links
            .iter()
            .map(|link| {
                format!(
                    "<a href=\"{}\" rel=\"noopener noreferrer\">{}</a>",
                    escape(&link.url),
                    escape(&link.label)
                )
            })
            .collect::<String>();
        slot(&mut public, "data-brand-links>", "</nav>", &links)?;
        // Only validated hex colors enter CSS. No URLs or raw CSS configuration.
        let css = format!(
            ":root{{--accent:{};--lime:{};--brand-mark-size:{}px}}\n:root[data-theme=dark]{{--accent:{}}}\n.public-hero h1 em{{color:var(--accent)}}\n.brand{{min-width:0}}.brand-wordmark{{min-width:0;overflow-wrap:anywhere}}.brand-mark{{flex-shrink:0;line-height:1;font-size:var(--brand-mark-size)}}\n[data-brand]{{overflow-wrap:anywhere}}.public-footer nav,.public-footer nav a{{min-width:0;max-width:100%;overflow-wrap:anywhere}}\n",
            config.accent,
            config.highlight,
            if config.mark.chars().count() > 1 {
                16
            } else {
                28
            },
            config.highlight
        );
        Ok(Self {
            public,
            dashboard,
            css,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn validates_bounds_urls_and_readable_colors() {
        BrandConfig::default().validate().unwrap();
        let example: crate::Config =
            toml::from_str(include_str!("../../../deploy/brand.example.toml")).unwrap();
        example.brand.validate().unwrap();
        assert_eq!(example.brand.name, "Orbit AI");
        assert_eq!(example.brand.links.len(), 2);
        for link in [
            "javascript:alert(1)",
            "//other.example",
            "http://other.example",
            "https://user:password@example.com",
            "https://example.com/\nsecret",
            "https://example.com\\@other",
        ] {
            assert!(
                BrandConfig {
                    documentation_url: link.into(),
                    ..Default::default()
                }
                .validate()
                .is_err()
            );
        }
        for color in ["red", "#fff", "#000000;url(https://x)", "#ffffff"] {
            assert!(
                BrandConfig {
                    accent: color.into(),
                    ..Default::default()
                }
                .validate()
                .is_err()
            );
        }
        assert!(
            BrandConfig {
                highlight: "#000000".into(),
                ..Default::default()
            }
            .validate()
            .is_err()
        );
        assert!(
            BrandConfig {
                wordmark: "a".repeat(25),
                ..Default::default()
            }
            .validate()
            .is_err()
        );
        assert!(toml::from_str::<BrandConfig>("acccent = '#123456'").is_err());
    }

    #[test]
    fn separate_brands_are_escaped_and_keep_status_and_csp_assets() {
        let one = BrandConfig {
            name: "Orbit <script>alert(1)</script> & Co".into(),
            wordmark: "Orbit & Co".into(),
            mark: "O".into(),
            accent: "#124b70".into(),
            highlight: "#b7e7ff".into(),
            links: vec![BrandLink {
                label: "Docs <b>safe</b>".into(),
                url: "https://docs.example.com/?a=1&b=2".into(),
            }],
            ..Default::default()
        };
        let rendered = RenderedBrand::new(&one).unwrap();
        let default = RenderedBrand::new(&BrandConfig::default()).unwrap();
        for html in [&rendered.public, &rendered.dashboard] {
            assert!(html.contains("Orbit &lt;script&gt;alert(1)&lt;/script&gt; &amp; Co"));
            assert!(!html.contains("<script>alert(1)</script>"));
            assert!(html.contains("/assets/brand.css"));
        }
        assert!(rendered.public.contains("Docs &lt;b&gt;safe&lt;/b&gt;"));
        assert!(rendered.public.contains("billing are not active yet"));
        assert!(!rendered.public.contains("https://shippin.cloud"));
        assert!(default.public.contains("https://shippin.cloud"));
        assert!(!default.public.contains("Orbit"));
        assert!(rendered.css.contains("--accent:#124b70"));
        assert!(!rendered.css.contains("url("));
        assert!(!default.css.contains("#124b70"));
    }
}
