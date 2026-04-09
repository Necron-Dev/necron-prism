#![cfg(feature = "http-api")]

use reqwest::{StatusCode, Url};
use serde::Deserialize;
use anyhow::{anyhow, Result};

use prism::config::ApiConfig;
use crate::proxy::routing::{JoinDecision, JoinTarget};

use super::types::TrafficBody;

pub struct ApiClient {
    inner: reqwest::Client,
    join_url: Url,
    traffic_url: Url,
    closed_url: Url,
}

impl ApiClient {
    pub fn new(config: &ApiConfig) -> Result<Self> {
        let mut headers = reqwest::header::HeaderMap::new();
        if let Some(token) = &config.bearer_token {
            let mut bearer = String::with_capacity(token.len() + 7);
            bearer.push_str("Bearer ");
            bearer.push_str(token);
            let mut value = reqwest::header::HeaderValue::from_str(&bearer)
                .map_err(|error| anyhow!("invalid api.bearer_token header: {error}"))?;
            value.set_sensitive(true);
            headers.insert(reqwest::header::AUTHORIZATION, value);
        }

        let inner = reqwest::Client::builder()
            .default_headers(headers)
            .timeout(std::time::Duration::from_millis(config.timeout_ms))
            .build()
            .map_err(|error| anyhow!("build reqwest client: {error}"))?;
        let base_url = config
            .base_url
            .as_deref()
            .ok_or_else(|| anyhow!("http api requires api.base_url"))?;
        let base_url = Url::parse(base_url)
            .map_err(|error| anyhow!("invalid api.base_url: {error}"))?;

        Ok(Self {
            join_url: base_url
                .join("v1/join")
                .map_err(|error| anyhow!("invalid join url: {error}"))?,
            traffic_url: base_url
                .join("v1/traffic")
                .map_err(|error| anyhow!("invalid traffic url: {error}"))?,
            closed_url: base_url
                .join("v1/closed")
                .map_err(|error| anyhow!("invalid closed url: {error}"))?,
            inner,
        })
    }

    pub async fn join(
        &self,
        name: Option<&str>,
        uuid: Option<&str>,
        src: Option<&str>,
        load: i32,
    ) -> Result<JoinDecision> {
        let load = load.to_string();
        let response = self
            .inner
            .get(self.join_url.as_str())
            .query(&[
                ("name", name.unwrap_or_default()),
                ("uuid", uuid.unwrap_or_default()),
                ("src", src.unwrap_or_default()),
                ("load", load.as_str()),
            ])
            .send()
            .await?;

        match response.status() {
            StatusCode::OK => {
                let body = response.json::<JoinOkResponse>().await?;
                Ok(JoinDecision::Allow(JoinTarget {
                    target_addr: body.data.target_addr,
                    rewrite_addr: body.data.rewrite_addr,
                    connection_id: body.data.connection_id,
                }))
            }
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN | StatusCode::SERVICE_UNAVAILABLE => {
                let body = response.json::<JoinDenyResponse>().await?;
                Ok(JoinDecision::Deny {
                    kick_reason: body.data.kick_reason,
                })
            }
            _ => Ok(JoinDecision::Deny {
                kick_reason: format!("join API returned {}", response.status()),
            }),
        }
    }

    pub async fn traffic(
        &self,
        connection_id: &str,
        send_bytes: u64,
        recv_bytes: u64,
    ) -> Result<Vec<String>> {
        let response = self
            .inner
            .post(self.traffic_url.as_str())
            .json(&std::collections::BTreeMap::from([(
                connection_id.to_owned(),
                TrafficBody {
                    send_bytes,
                    recv_bytes,
                },
            )]))
            .send()
            .await?;

        let response = response.error_for_status()?;
        let body = response.json::<TrafficResponse>().await?;
        Ok(body.data.connections_to_close)
    }

    pub async fn closed(&self, cid: &str, send: u64, recv: u64) -> Result<()> {
        let send = send.to_string();
        let recv = recv.to_string();

        self.inner
            .get(self.closed_url.as_str())
            .query(&[("cid", cid), ("send", send.as_str()), ("recv", recv.as_str())])
            .send()
            .await?
            .error_for_status()?;
        Ok(())
    }
}

#[derive(Debug, Deserialize)]
struct JoinOkResponse {
    data: JoinOkData,
}

#[derive(Debug, Deserialize)]
struct JoinOkData {
    target_addr: String,
    rewrite_addr: Option<String>,
    connection_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct JoinDenyResponse {
    data: JoinDenyData,
}

#[derive(Debug, Deserialize)]
struct JoinDenyData {
    kick_reason: String,
}

#[derive(Debug, Deserialize)]
struct TrafficResponse {
    data: TrafficResponseData,
}

#[derive(Debug, Deserialize)]
struct TrafficResponseData {
    connections_to_close: Vec<String>,
}
