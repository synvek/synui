//! Localization context using Project Fluent.

use fluent_bundle::{FluentBundle, FluentResource};
use fluent_langneg::negotiate_languages;
use std::collections::HashMap;
use thiserror::Error;
use unic_langid::LanguageIdentifier;

#[derive(Debug, Error)]
pub enum I18nError {
    #[error("locale error: {0}")]
    Locale(String),
    #[error("bundle error: {0}")]
    Bundle(String),
}

/// Holds Fluent bundles per locale.
pub struct L10n {
    bundles: HashMap<String, FluentBundle<FluentResource>>,
    current: String,
    default_locale: String,
}

impl L10n {
    pub fn new(default_locale: &str) -> Self {
        Self {
            bundles: HashMap::new(),
            current: default_locale.to_string(),
            default_locale: default_locale.to_string(),
        }
    }

    pub fn add_bundle(&mut self, locale: &str, ftl: &str) -> Result<(), I18nError> {
        let langid: LanguageIdentifier = locale
            .parse()
            .map_err(|e: unic_langid::LanguageIdentifierError| I18nError::Locale(e.to_string()))?;
        let resource = FluentResource::try_new(ftl.to_string())
            .map_err(|(_, errors)| I18nError::Bundle(format!("{errors:?}")))?;
        let mut bundle = FluentBundle::new(vec![langid]);
        bundle
            .add_resource(resource)
            .map_err(|e| I18nError::Bundle(format!("{e:?}")))?;
        self.bundles.insert(locale.to_string(), bundle);
        Ok(())
    }

    pub fn set_locale(&mut self, locale: &str) {
        if self.bundles.contains_key(locale) {
            self.current = locale.to_string();
        } else if self.bundles.contains_key(&self.default_locale) {
            self.current = self.default_locale.clone();
        }
    }

    pub fn locale(&self) -> &str {
        &self.current
    }

    pub fn is_rtl(&self) -> bool {
        matches!(
            self.current.as_str(),
            "ar" | "ar-SA" | "he" | "he-IL" | "fa" | "fa-IR" | "ur" | "ur-PK"
        )
    }

    pub fn translate(&self, key: &str) -> String {
        self.translate_with_args(key, &[])
    }

    pub fn translate_with_args(
        &self,
        key: &str,
        args: &[(&str, fluent_bundle::FluentValue<'_>)],
    ) -> String {
        use fluent_bundle::FluentArgs;
        let bundle = match self.bundles.get(&self.current) {
            Some(b) => b,
            None => self
                .bundles
                .get(&self.default_locale)
                .unwrap_or_else(|| self.fallback_key(key)),
        };
        let msg = match bundle.get_message(key) {
            Some(m) => m,
            None => return key.to_string(),
        };
        let pattern = match msg.value() {
            Some(p) => p,
            None => return key.to_string(),
        };
        let mut fluent_args = FluentArgs::new();
        for (name, value) in args {
            fluent_args.set(*name, value.clone());
        }
        let mut errors = vec![];
        bundle
            .format_pattern(pattern, Some(&fluent_args), &mut errors)
            .to_string()
    }

    fn fallback_key(&self, _key: &str) -> &FluentBundle<FluentResource> {
        self.bundles
            .get(&self.default_locale)
            .expect("no bundles loaded")
    }

    pub fn negotiate(preferred: &[&str], available: &[&str]) -> Option<String> {
        let preferred: Vec<LanguageIdentifier> =
            preferred.iter().filter_map(|s| s.parse().ok()).collect();
        let available: Vec<LanguageIdentifier> =
            available.iter().filter_map(|s| s.parse().ok()).collect();
        negotiate_languages(
            &preferred,
            &available,
            None,
            fluent_langneg::NegotiationStrategy::Filtering,
        )
        .first()
        .map(|l| l.to_string())
    }
}

#[macro_export]
macro_rules! t {
    ($l10n:expr, $key:expr) => {
        $l10n.translate($key)
    };
}
