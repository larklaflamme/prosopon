//! Web search client (Tavily or Brave).
//!
//! The provider is selected by `web_search.use` in `config.yaml` (`"tavily"`
//! or `"brave"`). Both return a normalized `Vec<SearchResult>` so the rest of
//! the pipeline is provider-agnostic.

use crate::config::WebSearchConfig;
use serde::Deserialize;

/// A single normalized search result.
#[derive(Debug, Clone)]
pub struct SearchResult {
    pub title: String,
    pub url: String,
    pub snippet: String,
}

/// HTTP client for web search.
pub struct WebSearchClient {
    client: reqwest::Client,
    provider: String,
    tavily_api_key: String,
    brave_api_key: String,
}

/// Errors that can occur while searching.
#[derive(Debug)]
pub enum WebSearchError {
    Http(reqwest::Error),
    /// The configured provider is neither "tavily" nor "brave".
    UnknownProvider(String),
    /// The provider is selected but its API key is empty.
    MissingApiKey(String),
}

impl std::fmt::Display for WebSearchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WebSearchError::Http(e) => write!(f, "web search request failed: {e}"),
            WebSearchError::UnknownProvider(p) => {
                write!(f, "unknown web search provider: {p} (expected tavily or brave)")
            }
            WebSearchError::MissingApiKey(p) => {
                write!(f, "web search provider {p} selected but its API key is empty")
            }
        }
    }
}

impl std::error::Error for WebSearchError {}

impl From<reqwest::Error> for WebSearchError {
    fn from(e: reqwest::Error) -> Self {
        WebSearchError::Http(e)
    }
}

// --- Tavily wire types ---

#[derive(Debug, Deserialize)]
struct TavilyResponse {
    #[serde(default)]
    results: Vec<TavilyResult>,
}

#[derive(Debug, Deserialize)]
struct TavilyResult {
    #[serde(default)]
    title: String,
    #[serde(default)]
    url: String,
    #[serde(default)]
    content: String,
}

// --- Brave wire types ---

#[derive(Debug, Deserialize)]
struct BraveResponse {
    #[serde(default)]
    web: BraveWeb,
}

#[derive(Debug, Deserialize, Default)]
struct BraveWeb {
    #[serde(default)]
    results: Vec<BraveResult>,
}

#[derive(Debug, Deserialize)]
struct BraveResult {
    #[serde(default)]
    title: String,
    #[serde(default)]
    url: String,
    #[serde(default)]
    description: String,
}

impl WebSearchClient {
    /// Build a client from web-search configuration.
    pub fn new(config: &WebSearchConfig) -> Self {
        Self {
            client: reqwest::Client::new(),
            provider: config.provider.clone(),
            tavily_api_key: config.tavily_api_key.clone(),
            brave_api_key: config.brave_api_key.clone(),
        }
    }

    /// Search the web, dispatching to the configured provider.
    pub async fn search(&self, query: &str) -> Result<Vec<SearchResult>, WebSearchError> {
        match self.provider.as_str() {
            "tavily" => self.search_tavily(query).await,
            "brave" => self.search_brave(query).await,
            other => Err(WebSearchError::UnknownProvider(other.to_string())),
        }
    }

    async fn search_tavily(&self, query: &str) -> Result<Vec<SearchResult>, WebSearchError> {
        if self.tavily_api_key.is_empty() {
            return Err(WebSearchError::MissingApiKey("tavily".into()));
        }
        let body = serde_json::json!({
            "api_key": self.tavily_api_key,
            "query": query,
            "search_depth": "basic",
            "max_results": 5,
        });
        let resp = self
            .client
            .post("https://api.tavily.com/search")
            .json(&body)
            .send()
            .await?
            .error_for_status()?;
        let parsed: TavilyResponse = resp.json().await?;
        Ok(parsed
            .results
            .into_iter()
            .map(|r| SearchResult {
                title: r.title,
                url: r.url,
                snippet: r.content,
            })
            .collect())
    }

    async fn search_brave(&self, query: &str) -> Result<Vec<SearchResult>, WebSearchError> {
        if self.brave_api_key.is_empty() {
            return Err(WebSearchError::MissingApiKey("brave".into()));
        }
        let resp = self
            .client
            .get("https://api.search.brave.com/res/v1/web/search")
            .query(&[("q", query), ("count", "5")])
            .header("X-Subscription-Token", &self.brave_api_key)
            .header("Accept", "application/json")
            .send()
            .await?
            .error_for_status()?;
        let parsed: BraveResponse = resp.json().await?;
        Ok(parsed
            .web
            .results
            .into_iter()
            .map(|r| SearchResult {
                title: r.title,
                url: r.url,
                snippet: r.description,
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tavily_response_deserializes() {
        let json = r#"{"results":[{"title":"T","url":"https://x","content":"body"}]}"#;
        let parsed: TavilyResponse = serde_json::from_str(json).expect("parse");
        assert_eq!(parsed.results.len(), 1);
        assert_eq!(parsed.results[0].title, "T");
    }

    #[test]
    fn brave_response_deserializes() {
        let json = r#"{"web":{"results":[{"title":"T","url":"https://x","description":"d"}]}}"#;
        let parsed: BraveResponse = serde_json::from_str(json).expect("parse");
        assert_eq!(parsed.web.results.len(), 1);
        assert_eq!(parsed.web.results[0].description, "d");
    }
}
