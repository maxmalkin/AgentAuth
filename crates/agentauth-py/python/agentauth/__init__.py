"""
AgentAuth Python SDK

This package provides Python bindings for the AgentAuth SDK, allowing
Python-based AI agents to authenticate with AgentAuth-enabled services.

Example usage:

    from agentauth import AgentAuthClient, Capability, BehavioralEnvelope

    # Create client
    client = AgentAuthClient(
        registry_url="https://registry.agentauth.dev",
        manifest_json=manifest_json,
        private_key=private_key_hex,
    )

    # Register agent
    await client.register()

    # Request grant
    capabilities = [Capability.read("calendar")]
    envelope = BehavioralEnvelope.default_restrictive()
    grant = await client.request_grant(
        service_provider_id="...",
        capabilities=capabilities,
        envelope=envelope,
    )

    # Get authentication headers
    headers = await client.authenticate_headers(
        service_provider_id="...",
        method="GET",
        url="https://api.example.com/calendar",
    )
"""

# Import from the Rust extension
from agentauth.agentauth import (
    AgentAuthClient,
    BehavioralEnvelope,
    Capability,
    CapabilityGrant,
    WriteConditions,
    __version__,
)

__all__ = [
    "AgentAuthClient",
    "BehavioralEnvelope",
    "Capability",
    "CapabilityGrant",
    "WriteConditions",
    "__version__",
]
