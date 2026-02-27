# AgentAuth Python SDK

Python bindings for the AgentAuth authentication system.

## Installation

```bash
pip install agentauth
```

## Quick Start

```python
import asyncio
from agentauth import AgentAuthClient, Capability, BehavioralEnvelope

async def main():
    # Create client with your agent's credentials
    client = AgentAuthClient(
        registry_url="https://registry.agentauth.dev",
        manifest_json=manifest_json,
        private_key=private_key_hex,
    )

    # Register the agent
    await client.register()

    # Request capabilities from a service provider
    capabilities = [
        Capability.read("calendar"),
        Capability.write("calendar"),
    ]
    envelope = BehavioralEnvelope.default_restrictive()

    grant = await client.request_grant(
        service_provider_id="your-service-provider-id",
        capabilities=capabilities,
        envelope=envelope,
    )

    # Get authentication headers for requests
    headers = await client.authenticate_headers(
        service_provider_id="your-service-provider-id",
        method="GET",
        url="https://api.example.com/calendar",
    )

    # Use headers in your HTTP requests
    # requests.get("https://api.example.com/calendar", headers=headers)

asyncio.run(main())
```

## Capabilities

AgentAuth supports the following capability types:

- **Read**: Read-only access to a resource
- **Write**: Write access to a resource (with optional conditions)
- **Transact**: Financial/irreversible transactions (requires two-step approval)
- **Delete**: Deletion capability (requires two-step approval)
- **Custom**: Custom namespace-scoped capabilities

### Examples

```python
from agentauth import Capability, WriteConditions

# Read capability with filter
read_cap = Capability.read("emails", filter="unread")

# Write capability with conditions
conditions = WriteConditions(
    filter="owner:self",
    max_items_per_request=10,
    append_only=True,
)
write_cap = Capability.write("documents", conditions=conditions)

# Transaction capability with currency
transact_cap = Capability.transact("payments", max_value=1000, currency="USD")

# Delete capability
delete_cap = Capability.delete("files", filter="temporary")

# Custom capability
custom_cap = Capability.custom(
    namespace="com.example",
    name="my_action",
    params={"key": "value"},
)
```

## Behavioral Envelopes

Behavioral envelopes define rate limits and constraints for agent actions:

```python
from agentauth import BehavioralEnvelope

# Default restrictive envelope (recommended)
envelope = BehavioralEnvelope.default_restrictive()

# Custom envelope
envelope = BehavioralEnvelope(
    max_requests_per_minute=100,
    max_burst=20,
    requires_human_online=True,
    human_confirmation_threshold=500,
    max_session_duration_secs=1800,
)
```

## Framework Integrations

### LangChain

```python
from agentauth import AgentAuthClient, Capability
from agentauth.integrations.langchain import AgentAuthToolkit

client = AgentAuthClient(...)
toolkit = AgentAuthToolkit(
    client=client,
    service_provider_id="your-service-provider-id",
)

# Wrap your tools with authentication
authenticated_tools = toolkit.wrap_tools([your_tool])
```

### AutoGen

```python
from agentauth import AgentAuthClient
from agentauth.integrations.autogen import AgentAuthMiddleware

client = AgentAuthClient(...)
middleware = AgentAuthMiddleware(
    client=client,
    service_provider_id="your-service-provider-id",
)

# Wrap LLM config with authentication
config = middleware.wrap_config(your_llm_config)
```

## License

MIT OR Apache-2.0
