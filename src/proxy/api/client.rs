use std::collections::BTreeMap;

use reqwest::StatusCode;
use serde::{Deserialize, Serialize};

use super::types::{JoinDecision, JoinTarget, TrafficSnapshot};
use crate::proxy::config::ApiConfig;

pub struct ApiClient {
    inner: reqwest::Client,
    base_url: String,
}

impl ApiClient {
    pub fn new(config: &ApiConfig) -> Result<Self, String> {
        let mut headers = reqwest::header::HeaderMap::new();
        if let Some(token) = &config.bearer_token {
            let mut value = reqwest::header::HeaderValue::from_str(&format!("Bearer {token}"))
                .map_err(|error| format!("invalid api.bearer_token header: {error}"))?;
            value.set_sensitive(true);
            headers.insert(reqwest::header::AUTHORIZATION, value);
        }

        let inner = reqwest::Client::builder()
            .default_headers(headers)
            .timeout(config.timeout)
            .build()
            .map_err(|error| format!("build reqwest client: {error}"))?;
        let base_url = config
            .base_url
            .clone()
            .ok_or_else(|| "http api requires api.base_url".to_string())?;

        Ok(Self { inner, base_url })
    }

    pub async fn join(
        &self,
        name: Option<&str>,
        uuid: Option<&str>,
        src: Option<&str>,
        load: i32,
    ) -> Result<JoinDecision, reqwest::Error> {
        let response = self
            .inner
            .get(format!("{}/v1/join", self.base_url))
            .query(&[
                ("name", name.unwrap_or_default()),
                ("uuid", uuid.unwrap_or_default()),
                ("src", src.unwrap_or_default()),
                ("load", &load.to_string()),
            ])
            .send()
            .await?;

        match response.status() {
            StatusCode::OK => {
                let body = response.json::<JoinOkResponse>().await?;
                Ok(JoinDecision::Allow(JoinTarget {
                    rewrite_addr: body
                        .data
                        .rewrite_addr
                        .unwrap_or_else(|| body.data.target_addr.clone()),
                    target_addr: body.data.target_addr,
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

    pub async fn traffic(&self, snapshot: &TrafficSnapshot) -> Result<Vec<String>, reqwest::Error> {
        let body = snapshot
            .entries
            .iter()
            .map(|(key, value)| {
                (
                    key.clone(),
                    TrafficBody {
                        send_bytes: value.send_bytes,
                        recv_bytes: value.recv_bytes,
                    },
                )
            })
            .collect::<BTreeMap<_, _>>();

        let response = self
            .inner
            .post(format!("{}/v1/traffic", self.base_url))
            .json(&body)
            .send()
            .await?;

        let response = response.error_for_status()?;
        let body = response.json::<TrafficResponse>().await?;
        Ok(body.data.connections_to_close)
    }

    pub async fn closed(&self, cid: &str, send: u64, recv: u64) -> Result<(), reqwest::Error> {
        self.inner
            .get(format!("{}/v1/closed", self.base_url))
            .query(&[
                ("cid", cid.to_string()),
                ("send", send.to_string()),
                ("recv", recv.to_string()),
            ])
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
    connection_id: String,
}

#[derive(Debug, Deserialize)]
struct JoinDenyResponse {
    data: JoinDenyData,
}

#[derive(Debug, Deserialize)]
struct JoinDenyData {
    kick_reason: String,
}

#[derive(Debug, Serialize)]
struct TrafficBody {
    send_bytes: u64,
    recv_bytes: u64,
}

#[derive(Debug, Deserialize)]
struct TrafficResponse {
    data: TrafficResponseData,
}

#[derive(Debug, Deserialize)]
struct TrafficResponseData {
    connections_to_close: Vec<String>,
}
