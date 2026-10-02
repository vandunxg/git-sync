//! `DestinationApi` over the GitLab REST API (P06-T01).
//!
//! `GET /api/v4/projects/:id` (`:id` = numeric ID or URL-encoded full path) and
//! `POST /api/v4/projects` (`name`, `path`, `namespace_id`, `visibility`,
//! `description`, `initialize_with_readme`; 201 Created), per
//! docs.gitlab.com/api/projects (read 2026-10-02). Response fields used: `id`,
//! `path_with_namespace`, `namespace.id`, `visibility`, `http_url_to_repo`,
//! `description`, `empty_repo`, `permissions.{project,group}_access.access_level`.
//!
//! Every project in a response is validated: numeric IDs, a plain path, and a clone URL
//! that is exactly `<origin><path_with_namespace>.git` (no userinfo, no other host).
//! A 404 is "absent"; any other non-success is a typed error. Redirects are refused.

use gbm_application::destination::{CreateIntent, DestinationApi, ProjectView, Visibility};
use gbm_application::providers::{ProviderError, precheck_token};
use gbm_domain::contract::secret::SecretString;
use reqwest::StatusCode;
use serde::Deserialize;

use super::{GitLabClient, PRIVATE_TOKEN};
use crate::http::{HttpResponse, classify_failure};

const MAX_PATH: usize = 512;
const MAX_DESCRIPTION: usize = 2000;

#[derive(Deserialize)]
struct NamespaceRef {
    id: u64,
}

#[derive(Deserialize)]
struct Access {
    access_level: u32,
}

#[derive(Deserialize, Default)]
struct Permissions {
    project_access: Option<Access>,
    group_access: Option<Access>,
}

#[derive(Deserialize)]
struct ProjectBody {
    id: u64,
    path_with_namespace: String,
    namespace: NamespaceRef,
    visibility: String,
    http_url_to_repo: String,
    description: Option<String>,
    empty_repo: bool,
    #[serde(default)]
    permissions: Option<Permissions>,
}

/// GitLab namespace and project paths: `[A-Za-z0-9._-]` segments joined by `/`.
fn is_plain_path(path: &str) -> bool {
    !path.is_empty()
        && path.len() <= MAX_PATH
        && path.split('/').all(|segment| {
            !segment.is_empty()
                && segment != "."
                && segment != ".."
                && segment
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
        })
}

/// Destination operations for one token. The token is borrowed, never stored.
pub struct GitLabDestination<'a> {
    client: &'a GitLabClient,
    token: &'a SecretString,
}

impl<'a> GitLabDestination<'a> {
    pub fn new(client: &'a GitLabClient, token: &'a SecretString) -> Result<Self, ProviderError> {
        precheck_token(token.expose())?;
        Ok(Self { client, token })
    }

    fn project(&self, response: &HttpResponse) -> Result<ProjectView, ProviderError> {
        let body: ProjectBody = serde_json::from_slice(&response.body)
            .map_err(|_| ProviderError::UnexpectedResponse)?;
        let expected_url = format!(
            "{}{}.git",
            self.client.http.origin().as_str(),
            body.path_with_namespace
        );
        let description = body.description.unwrap_or_default();
        if !is_plain_path(&body.path_with_namespace)
            || !body.path_with_namespace.contains('/')
            || body.http_url_to_repo != expected_url
            || description.len() > MAX_DESCRIPTION
        {
            return Err(ProviderError::UnexpectedResponse);
        }
        let visibility =
            Visibility::parse(&body.visibility).ok_or(ProviderError::UnexpectedResponse)?;
        let permissions = body.permissions.unwrap_or_default();
        let access_level = [permissions.project_access, permissions.group_access]
            .into_iter()
            .flatten()
            .map(|a| a.access_level)
            .max();
        Ok(ProjectView {
            project_id: body.id,
            namespace_id: body.namespace.id,
            path_with_namespace: body.path_with_namespace,
            visibility,
            clone_url: body.http_url_to_repo,
            description,
            empty_repo: body.empty_repo,
            access_level,
        })
    }

    async fn get_project(&self, api_path: &str) -> Result<Option<ProjectView>, ProviderError> {
        let response = self
            .client
            .http
            .get(api_path, (PRIVATE_TOKEN, self.token.expose()), &[])
            .await?;
        if response.status == StatusCode::NOT_FOUND {
            return Ok(None);
        }
        if response.status != StatusCode::OK {
            return Err(classify_failure(&response));
        }
        self.project(&response).map(Some)
    }
}

impl DestinationApi for GitLabDestination<'_> {
    async fn find_by_path(&self, full_path: &str) -> Result<Option<ProjectView>, ProviderError> {
        if !is_plain_path(full_path) {
            return Err(ProviderError::UnexpectedResponse);
        }
        let encoded = full_path.replace('/', "%2F");
        let found = self
            .get_project(&format!("api/v4/projects/{encoded}"))
            .await?;
        match found {
            // A project answered for another path (e.g. a rename route) is not proof
            // that the exact path is free or ours: uncertain.
            Some(project) if !project.path_with_namespace.eq_ignore_ascii_case(full_path) => {
                Err(ProviderError::UnexpectedResponse)
            }
            other => Ok(other),
        }
    }

    async fn get_by_id(&self, project_id: u64) -> Result<Option<ProjectView>, ProviderError> {
        let found = self
            .get_project(&format!("api/v4/projects/{project_id}"))
            .await?;
        match found {
            Some(project) if project.project_id != project_id => {
                Err(ProviderError::UnexpectedResponse)
            }
            other => Ok(other),
        }
    }

    async fn create(&self, intent: &CreateIntent) -> Result<ProjectView, ProviderError> {
        if !is_plain_path(&intent.full_path()) || intent.path.contains('/') {
            return Err(ProviderError::UnexpectedResponse);
        }
        let body = serde_json::json!({
            "name": intent.path,
            "path": intent.path,
            "namespace_id": intent.namespace_id,
            "visibility": intent.visibility.as_str(),
            "description": intent.description(),
            "initialize_with_readme": false,
        });
        let body = serde_json::to_vec(&body).map_err(|_| ProviderError::UnexpectedResponse)?;
        let response = self
            .client
            .http
            .post_json(
                "api/v4/projects",
                (PRIVATE_TOKEN, self.token.expose()),
                body,
            )
            .await?;
        if response.status != StatusCode::CREATED {
            return Err(classify_failure(&response));
        }
        self.project(&response)
    }
}

#[cfg(test)]
mod tests;
