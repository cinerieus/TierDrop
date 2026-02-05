use reqwest::Client;

use super::models::{ControllerMember, ControllerNetwork, NodeStatus};

#[derive(Clone)]
pub struct ZtClient {
    client: Client,
    base_url: String,
    auth_token: String,
}

impl ZtClient {
    pub fn new(base_url: String, auth_token: String) -> Self {
        Self {
            client: Client::new(),
            base_url,
            auth_token,
        }
    }

    fn request(&self, path: &str) -> reqwest::RequestBuilder {
        self.client
            .get(format!("{}{}", self.base_url, path))
            .header("X-ZT1-Auth", &self.auth_token)
    }

    pub async fn get_status(&self) -> Result<NodeStatus, String> {
        self.request("/status")
            .send()
            .await
            .map_err(|e| format!("Failed to connect to ZeroTier: {}", e))?
            .json()
            .await
            .map_err(|e| format!("Failed to parse status: {}", e))
    }

    // ---- Controller Network methods ----

    pub async fn get_controller_networks(&self) -> Result<Vec<String>, String> {
        self.request("/controller/network")
            .send()
            .await
            .map_err(|e| format!("Failed to fetch controller networks: {}", e))?
            .json()
            .await
            .map_err(|e| format!("Failed to parse controller networks: {}", e))
    }

    pub async fn get_controller_network(&self, nwid: &str) -> Result<ControllerNetwork, String> {
        self.request(&format!("/controller/network/{}", nwid))
            .send()
            .await
            .map_err(|e| format!("Failed to fetch controller network: {}", e))?
            .json()
            .await
            .map_err(|e| format!("Failed to parse controller network: {}", e))
    }

    pub async fn create_controller_network(
        &self,
        node_id: &str,
    ) -> Result<ControllerNetwork, String> {
        self.client
            .post(format!(
                "{}/controller/network/{}______",
                self.base_url, node_id
            ))
            .header("X-ZT1-Auth", &self.auth_token)
            .json(&serde_json::json!({}))
            .send()
            .await
            .map_err(|e| format!("Failed to create network: {}", e))?
            .json()
            .await
            .map_err(|e| format!("Failed to parse create response: {}", e))
    }

    pub async fn update_controller_network(
        &self,
        nwid: &str,
        body: serde_json::Value,
    ) -> Result<ControllerNetwork, String> {
        self.client
            .post(format!("{}/controller/network/{}", self.base_url, nwid))
            .header("X-ZT1-Auth", &self.auth_token)
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("Failed to update network: {}", e))?
            .json()
            .await
            .map_err(|e| format!("Failed to parse update response: {}", e))
    }

    pub async fn delete_controller_network(&self, nwid: &str) -> Result<(), String> {
        let resp = self
            .client
            .delete(format!("{}/controller/network/{}", self.base_url, nwid))
            .header("X-ZT1-Auth", &self.auth_token)
            .send()
            .await
            .map_err(|e| format!("Failed to delete network: {}", e))?;
        if resp.status().is_success() {
            Ok(())
        } else {
            Err(format!(
                "Delete network failed with status: {}",
                resp.status()
            ))
        }
    }

    // ---- Controller Member methods ----

    pub async fn get_controller_members(
        &self,
        nwid: &str,
    ) -> Result<std::collections::HashMap<String, i64>, String> {
        self.request(&format!("/controller/network/{}/member", nwid))
            .send()
            .await
            .map_err(|e| format!("Failed to fetch members: {}", e))?
            .json()
            .await
            .map_err(|e| format!("Failed to parse members: {}", e))
    }

    pub async fn get_controller_member(
        &self,
        nwid: &str,
        member_id: &str,
    ) -> Result<ControllerMember, String> {
        self.request(&format!(
            "/controller/network/{}/member/{}",
            nwid, member_id
        ))
        .send()
        .await
        .map_err(|e| format!("Failed to fetch member: {}", e))?
        .json()
        .await
        .map_err(|e| format!("Failed to parse member: {}", e))
    }

    pub async fn update_controller_member(
        &self,
        nwid: &str,
        member_id: &str,
        body: serde_json::Value,
    ) -> Result<ControllerMember, String> {
        self.client
            .post(format!(
                "{}/controller/network/{}/member/{}",
                self.base_url, nwid, member_id
            ))
            .header("X-ZT1-Auth", &self.auth_token)
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("Failed to update member: {}", e))?
            .json()
            .await
            .map_err(|e| format!("Failed to parse member update: {}", e))
    }

    pub async fn delete_controller_member(
        &self,
        nwid: &str,
        member_id: &str,
    ) -> Result<(), String> {
        let resp = self
            .client
            .delete(format!(
                "{}/controller/network/{}/member/{}",
                self.base_url, nwid, member_id
            ))
            .header("X-ZT1-Auth", &self.auth_token)
            .send()
            .await
            .map_err(|e| format!("Failed to delete member: {}", e))?;
        if resp.status().is_success() {
            Ok(())
        } else {
            Err(format!(
                "Delete member failed with status: {}",
                resp.status()
            ))
        }
    }
}
