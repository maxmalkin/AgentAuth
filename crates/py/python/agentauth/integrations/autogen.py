"""
AutoGen integration for AgentAuth.

This module provides middleware that adds AgentAuth authentication
to AutoGen agent HTTP requests.
"""

from typing import Any, Callable, Dict, List, Optional
import asyncio
import functools

try:
    import autogen

    AUTOGEN_AVAILABLE = True
except ImportError:
    AUTOGEN_AVAILABLE = False


class AgentAuthMiddleware:
    """
    Middleware that adds AgentAuth authentication to AutoGen agents.

    This middleware intercepts HTTP requests made by AutoGen agents
    and adds the necessary AgentAuth headers.

    Example:
        from agentauth import AgentAuthClient
        from agentauth.integrations.autogen import AgentAuthMiddleware
        import autogen

        client = AgentAuthClient(...)
        middleware = AgentAuthMiddleware(client, service_provider_id="...")

        # Create an AutoGen agent with authentication
        config = middleware.wrap_config({
            "model": "gpt-4",
            "api_base": "https://api.example.com",
        })

        agent = autogen.AssistantAgent(
            name="assistant",
            llm_config=config,
        )
    """

    def __init__(
        self,
        client: Any,  # AgentAuthClient
        service_provider_id: str,
        capabilities: Optional[List[Any]] = None,
    ):
        """
        Initialize the middleware.

        Args:
            client: An AgentAuthClient instance
            service_provider_id: The service provider to authenticate with
            capabilities: Optional list of capabilities to request
        """
        if not AUTOGEN_AVAILABLE:
            raise ImportError(
                "AutoGen is not installed. Install with: pip install agentauth[autogen]"
            )

        self.client = client
        self.service_provider_id = service_provider_id
        self.capabilities = capabilities or []
        self._grant_obtained = False

    async def ensure_grant(self) -> None:
        """Ensure we have a grant for the service provider."""
        if self._grant_obtained:
            return

        if self.capabilities:
            from agentauth import BehavioralEnvelope

            await self.client.request_grant(
                service_provider_id=self.service_provider_id,
                capabilities=self.capabilities,
                envelope=BehavioralEnvelope.default_restrictive(),
            )
        self._grant_obtained = True

    async def get_auth_headers(self, method: str, url: str) -> Dict[str, str]:
        """
        Get authentication headers for a request.

        Args:
            method: HTTP method
            url: Target URL

        Returns:
            Dict of headers to add
        """
        await self.ensure_grant()
        return await self.client.authenticate_headers(
            service_provider_id=self.service_provider_id,
            method=method,
            url=url,
        )

    def wrap_config(self, config: Dict[str, Any]) -> Dict[str, Any]:
        """
        Wrap an LLM config with AgentAuth authentication.

        This adds a request hook that automatically adds authentication
        headers to all requests.

        Args:
            config: The original LLM config dict

        Returns:
            Modified config with authentication
        """
        original_request_timeout = config.get("request_timeout", 60)
        middleware = self

        def get_headers_sync(method: str, url: str) -> Dict[str, str]:
            """Synchronously get auth headers."""
            loop = asyncio.new_event_loop()
            try:
                return loop.run_until_complete(middleware.get_auth_headers(method, url))
            finally:
                loop.close()

        # Add custom headers function
        config = config.copy()

        # Store original headers if any
        original_headers = config.get("headers", {})

        def get_combined_headers(method: str = "POST", url: str = "") -> Dict[str, str]:
            """Get combined headers including auth."""
            auth_headers = get_headers_sync(method, url or config.get("api_base", ""))
            return {**original_headers, **auth_headers}

        # AutoGen supports extra_headers in the config
        config["extra_headers"] = get_combined_headers

        return config

    def wrap_agent(self, agent_cls: type, *args, **kwargs) -> Any:
        """
        Create an AutoGen agent with authentication.

        Args:
            agent_cls: The AutoGen agent class
            *args: Arguments to pass to the agent
            **kwargs: Keyword arguments to pass to the agent

        Returns:
            An agent instance with authentication configured
        """
        if "llm_config" in kwargs:
            kwargs["llm_config"] = self.wrap_config(kwargs["llm_config"])
        return agent_cls(*args, **kwargs)


__all__ = ["AgentAuthMiddleware"]
