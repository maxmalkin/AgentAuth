//! Python bindings for the AgentAuth SDK.
//!
//! This module provides Python bindings using PyO3, allowing Python agents
//! to use the AgentAuth SDK for authentication.

// False positive: PyO3's PyResult with Bound return types triggers useless_conversion
#![allow(clippy::useless_conversion)]

use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyDict;

use agentauth_core::types::capability::WriteConditions;
use agentauth_core::types::{BehavioralEnvelope, Capability, ServiceProviderId, SignedManifest};
use agentauth_sdk::{AgentAuthClient, SdkConfig};

use std::collections::HashMap;
use std::sync::Arc;

/// Python-accessible WriteConditions type.
#[pyclass(name = "WriteConditions", frozen, from_py_object)]
#[derive(Clone)]
pub struct PyWriteConditions {
    inner: WriteConditions,
}

#[pymethods]
impl PyWriteConditions {
    /// Creates new write conditions.
    #[new]
    #[pyo3(signature = (filter=None, max_items_per_request=None, append_only=false))]
    fn new(filter: Option<String>, max_items_per_request: Option<u32>, append_only: bool) -> Self {
        Self {
            inner: WriteConditions {
                filter,
                max_items_per_request,
                append_only,
            },
        }
    }

    fn __repr__(&self) -> String {
        format!("{:?}", self.inner)
    }
}

/// Python-accessible Capability type.
#[pyclass(name = "Capability", frozen, from_py_object)]
#[derive(Clone)]
pub struct PyCapability {
    inner: Capability,
}

#[pymethods]
impl PyCapability {
    /// Creates a Read capability.
    #[staticmethod]
    #[pyo3(signature = (resource, filter=None))]
    fn read(resource: String, filter: Option<String>) -> Self {
        Self {
            inner: Capability::Read { resource, filter },
        }
    }

    /// Creates a Write capability.
    #[staticmethod]
    #[pyo3(signature = (resource, conditions=None))]
    fn write(resource: String, conditions: Option<PyWriteConditions>) -> Self {
        Self {
            inner: Capability::Write {
                resource,
                conditions: conditions.map(|c| c.inner),
            },
        }
    }

    /// Creates a Transact capability.
    #[staticmethod]
    #[pyo3(signature = (resource, max_value, currency=None))]
    fn transact(resource: String, max_value: u64, currency: Option<String>) -> Self {
        Self {
            inner: Capability::Transact {
                resource,
                max_value,
                currency,
            },
        }
    }

    /// Creates a Delete capability.
    #[staticmethod]
    #[pyo3(signature = (resource, filter=None))]
    fn delete(resource: String, filter: Option<String>) -> Self {
        Self {
            inner: Capability::Delete { resource, filter },
        }
    }

    /// Creates a Custom capability.
    ///
    /// The `params` dict values will be converted to JSON values.
    #[staticmethod]
    fn custom(
        py: Python<'_>,
        namespace: String,
        name: String,
        params: &Bound<'_, PyDict>,
    ) -> PyResult<Self> {
        let mut json_params: HashMap<String, serde_json::Value> = HashMap::new();
        for (key, value) in params.iter() {
            let key_str: String = key.extract()?;
            let json_value = python_to_json_value(py, &value)?;
            json_params.insert(key_str, json_value);
        }
        Ok(Self {
            inner: Capability::Custom {
                namespace,
                name,
                params: json_params,
            },
        })
    }

    fn __repr__(&self) -> String {
        format!("{:?}", self.inner)
    }
}

/// Converts a Python object to a serde_json::Value.
#[allow(deprecated)] // downcast is deprecated but cast has different ergonomics
fn python_to_json_value(_py: Python<'_>, obj: &Bound<'_, PyAny>) -> PyResult<serde_json::Value> {
    if obj.is_none() {
        return Ok(serde_json::Value::Null);
    }
    if let Ok(b) = obj.extract::<bool>() {
        return Ok(serde_json::Value::Bool(b));
    }
    if let Ok(i) = obj.extract::<i64>() {
        return Ok(serde_json::Value::Number(i.into()));
    }
    if let Ok(f) = obj.extract::<f64>() {
        if let Some(n) = serde_json::Number::from_f64(f) {
            return Ok(serde_json::Value::Number(n));
        }
    }
    if let Ok(s) = obj.extract::<String>() {
        return Ok(serde_json::Value::String(s));
    }
    if let Ok(list) = obj.downcast::<pyo3::types::PyList>() {
        let mut arr = Vec::new();
        for item in list.iter() {
            arr.push(python_to_json_value(_py, &item)?);
        }
        return Ok(serde_json::Value::Array(arr));
    }
    if let Ok(dict) = obj.downcast::<PyDict>() {
        let mut map = serde_json::Map::new();
        for (key, value) in dict.iter() {
            let key_str: String = key.extract()?;
            map.insert(key_str, python_to_json_value(_py, &value)?);
        }
        return Ok(serde_json::Value::Object(map));
    }
    // Fallback: convert to string
    Ok(serde_json::Value::String(obj.to_string()))
}

impl From<PyCapability> for Capability {
    fn from(py: PyCapability) -> Self {
        py.inner
    }
}

impl From<Capability> for PyCapability {
    fn from(cap: Capability) -> Self {
        Self { inner: cap }
    }
}

/// Python-accessible BehavioralEnvelope type.
#[pyclass(name = "BehavioralEnvelope", frozen, from_py_object)]
#[derive(Clone)]
pub struct PyBehavioralEnvelope {
    inner: BehavioralEnvelope,
}

#[pymethods]
impl PyBehavioralEnvelope {
    /// Creates a new behavioral envelope.
    #[new]
    #[pyo3(signature = (
        max_requests_per_minute=30,
        max_burst=5,
        requires_human_online=false,
        human_confirmation_threshold=None,
        max_session_duration_secs=3600
    ))]
    fn new(
        max_requests_per_minute: u32,
        max_burst: u32,
        requires_human_online: bool,
        human_confirmation_threshold: Option<u64>,
        max_session_duration_secs: u32,
    ) -> Self {
        Self {
            inner: BehavioralEnvelope {
                max_requests_per_minute,
                max_burst,
                requires_human_online,
                human_confirmation_threshold,
                allowed_time_windows: vec![],
                max_session_duration_secs,
            },
        }
    }

    /// Creates a default restrictive envelope.
    #[staticmethod]
    fn default_restrictive() -> Self {
        Self {
            inner: BehavioralEnvelope::default_restrictive(),
        }
    }

    /// Creates a default permissive envelope (for testing).
    #[staticmethod]
    fn default_permissive() -> Self {
        Self {
            inner: BehavioralEnvelope::default_permissive(),
        }
    }

    /// Returns a human-readable description of the envelope.
    fn to_human_readable(&self) -> String {
        self.inner.to_human_readable()
    }

    fn __repr__(&self) -> String {
        format!("{:?}", self.inner)
    }
}

impl From<PyBehavioralEnvelope> for BehavioralEnvelope {
    fn from(py: PyBehavioralEnvelope) -> Self {
        py.inner
    }
}

impl From<BehavioralEnvelope> for PyBehavioralEnvelope {
    fn from(env: BehavioralEnvelope) -> Self {
        Self { inner: env }
    }
}

/// Python-accessible grant type.
#[pyclass(name = "CapabilityGrant")]
pub struct PyCapabilityGrant {
    /// The grant ID.
    #[pyo3(get)]
    pub grant_id: String,
    /// Service provider ID.
    #[pyo3(get)]
    pub service_provider_id: String,
    /// Granted capabilities.
    pub capabilities: Vec<PyCapability>,
    /// Behavioral envelope.
    pub envelope: PyBehavioralEnvelope,
}

#[pymethods]
impl PyCapabilityGrant {
    /// Returns the granted capabilities.
    fn get_capabilities(&self) -> Vec<PyCapability> {
        self.capabilities.clone()
    }

    /// Returns the behavioral envelope.
    fn get_envelope(&self) -> PyBehavioralEnvelope {
        self.envelope.clone()
    }

    fn __repr__(&self) -> String {
        format!(
            "CapabilityGrant(grant_id='{}', service_provider_id='{}')",
            self.grant_id, self.service_provider_id
        )
    }
}

/// Python-accessible AgentAuth client.
#[pyclass(name = "AgentAuthClient")]
pub struct PyAgentAuthClient {
    client: Arc<AgentAuthClient>,
}

#[pymethods]
impl PyAgentAuthClient {
    /// Creates a new AgentAuth client.
    ///
    /// # Arguments
    ///
    /// * `registry_url` - URL of the AgentAuth registry
    /// * `manifest_json` - JSON-serialized signed manifest
    /// * `private_key` - 32-byte Ed25519 private key (hex or base64 encoded)
    #[new]
    fn new(registry_url: &str, manifest_json: &str, private_key: &str) -> PyResult<Self> {
        // Parse the configuration
        let config = SdkConfig::new(registry_url)
            .map_err(|e| PyValueError::new_err(format!("Invalid registry URL: {e}")))?;

        // Parse the manifest
        let manifest: SignedManifest = serde_json::from_str(manifest_json)
            .map_err(|e| PyValueError::new_err(format!("Invalid manifest JSON: {e}")))?;

        // Parse the private key
        let key_bytes = parse_private_key(private_key)?;

        // Create the client
        let client = AgentAuthClient::new(config, manifest, &key_bytes)
            .map_err(|e| PyRuntimeError::new_err(format!("Failed to create client: {e}")))?;

        Ok(Self {
            client: Arc::new(client),
        })
    }

    /// Returns the agent ID.
    fn agent_id(&self) -> String {
        self.client.agent_id().to_string()
    }

    /// Registers the agent with the registry.
    fn register<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let client = self.client.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            client
                .register()
                .await
                .map_err(|e| PyRuntimeError::new_err(format!("Registration failed: {e}")))
        })
    }

    /// Requests a capability grant from a service provider.
    fn request_grant<'py>(
        &self,
        py: Python<'py>,
        service_provider_id: &str,
        capabilities: Vec<PyCapability>,
        envelope: PyBehavioralEnvelope,
    ) -> PyResult<Bound<'py, PyAny>> {
        let client = self.client.clone();
        let sp_id = parse_service_provider_id(service_provider_id)?;
        let caps: Vec<Capability> = capabilities.into_iter().map(|c| c.into()).collect();
        let env: BehavioralEnvelope = envelope.into();

        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let grant = client
                .request_grant(sp_id, caps, env)
                .await
                .map_err(|e| PyRuntimeError::new_err(format!("Grant request failed: {e}")))?;

            Ok(PyCapabilityGrant {
                grant_id: grant.grant_id,
                service_provider_id: sp_id.to_string(),
                capabilities: grant.capabilities.into_iter().map(|c| c.into()).collect(),
                envelope: grant.envelope.into(),
            })
        })
    }

    /// Gets an access token for a service provider.
    fn get_token<'py>(
        &self,
        py: Python<'py>,
        service_provider_id: &str,
    ) -> PyResult<Bound<'py, PyAny>> {
        let client = self.client.clone();
        let sp_id = parse_service_provider_id(service_provider_id)?;

        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            client
                .get_token(&sp_id)
                .await
                .map_err(|e| PyRuntimeError::new_err(format!("Token retrieval failed: {e}")))
        })
    }

    /// Authenticates a request by returning headers to add.
    ///
    /// Returns a dict with 'Authorization' and 'AgentDPoP' headers.
    fn authenticate_headers<'py>(
        &self,
        py: Python<'py>,
        service_provider_id: &str,
        method: &str,
        url: &str,
    ) -> PyResult<Bound<'py, PyAny>> {
        let client = self.client.clone();
        let sp_id = parse_service_provider_id(service_provider_id)?;
        let method = method.to_string();
        let url = url.to_string();

        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let mut headers = reqwest::header::HeaderMap::new();
            client
                .authenticate_request(&sp_id, &method, &url, &mut headers)
                .await
                .map_err(|e| PyRuntimeError::new_err(format!("Authentication failed: {e}")))?;

            // Convert headers to a HashMap to return to Python
            let mut result: HashMap<String, String> = HashMap::new();
            for (key, value) in &headers {
                result.insert(key.as_str().to_string(), value.to_str().unwrap_or("").to_string());
            }
            Ok(result)
        })
    }

    fn __repr__(&self) -> String {
        format!(
            "AgentAuthClient(agent_id='{}', registry_url='{}')",
            self.client.agent_id(),
            self.client.registry_url()
        )
    }
}

/// Parse a service provider ID from string.
fn parse_service_provider_id(s: &str) -> PyResult<ServiceProviderId> {
    let uuid = uuid::Uuid::parse_str(s)
        .map_err(|e| PyValueError::new_err(format!("Invalid service provider ID: {e}")))?;
    Ok(ServiceProviderId::from_uuid(uuid))
}

/// Parse a private key from hex or base64.
fn parse_private_key(s: &str) -> PyResult<[u8; 32]> {
    // Try hex first
    if s.len() == 64 {
        let bytes =
            hex::decode(s).map_err(|e| PyValueError::new_err(format!("Invalid hex key: {e}")))?;
        if bytes.len() != 32 {
            return Err(PyValueError::new_err("Key must be 32 bytes"));
        }
        let mut arr = [0u8; 32];
        arr.copy_from_slice(&bytes);
        return Ok(arr);
    }

    // Try base64
    use base64::{engine::general_purpose::STANDARD, Engine};
    let bytes = STANDARD
        .decode(s)
        .map_err(|e| PyValueError::new_err(format!("Invalid base64 key: {e}")))?;
    if bytes.len() != 32 {
        return Err(PyValueError::new_err("Key must be 32 bytes"));
    }
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&bytes);
    Ok(arr)
}

/// Python AgentAuth SDK error type.
#[pyclass(name = "AgentAuthError", extends = pyo3::exceptions::PyException)]
pub struct PyAgentAuthError;

/// The agentauth Python module.
#[pymodule]
fn agentauth(m: &Bound<'_, PyModule>) -> PyResult<()> {
    // Types
    m.add_class::<PyWriteConditions>()?;
    m.add_class::<PyCapability>()?;
    m.add_class::<PyBehavioralEnvelope>()?;
    m.add_class::<PyCapabilityGrant>()?;
    m.add_class::<PyAgentAuthClient>()?;

    // Add module docstring
    m.add("__doc__", "Python bindings for the AgentAuth SDK")?;
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;

    Ok(())
}
