"""
LangChain integration for AgentAuth.

This module provides an AgentAuthToolkit that wraps LangChain tools
with AgentAuth authentication.
"""

from typing import Any, Callable, List, Optional, TypeVar
import asyncio
import functools

try:
    from langchain_core.tools import BaseTool, Tool
    from langchain_core.callbacks import CallbackManagerForToolRun

    LANGCHAIN_AVAILABLE = True
except ImportError:
    LANGCHAIN_AVAILABLE = False
    BaseTool = object
    Tool = object
    CallbackManagerForToolRun = object


class AgentAuthToolkit:
    """
    A toolkit that wraps LangChain tools with AgentAuth authentication.

    This toolkit intercepts tool calls and adds AgentAuth headers to any
    HTTP requests made by the tools.

    Example:
        from agentauth import AgentAuthClient, Capability
        from agentauth.integrations.langchain import AgentAuthToolkit
        from langchain_community.tools import RequestsTool

        client = AgentAuthClient(...)
        toolkit = AgentAuthToolkit(
            client=client,
            service_provider_id="...",
        )

        # Wrap tools with authentication
        http_tool = RequestsTool()
        authenticated_tools = toolkit.wrap_tools([http_tool])
    """

    def __init__(
        self,
        client: Any,  # AgentAuthClient
        service_provider_id: str,
        capabilities: Optional[List[Any]] = None,
    ):
        """
        Initialize the AgentAuth toolkit.

        Args:
            client: An AgentAuthClient instance
            service_provider_id: The service provider to authenticate with
            capabilities: Optional list of capabilities to request (if grant not already obtained)
        """
        if not LANGCHAIN_AVAILABLE:
            raise ImportError(
                "LangChain is not installed. Install with: pip install agentauth[langchain]"
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

    async def get_auth_headers(self, method: str, url: str) -> dict:
        """
        Get authentication headers for a request.

        Args:
            method: HTTP method (GET, POST, etc.)
            url: Target URL

        Returns:
            Dict of headers to add to the request
        """
        await self.ensure_grant()
        return await self.client.authenticate_headers(
            service_provider_id=self.service_provider_id,
            method=method,
            url=url,
        )

    def wrap_tools(self, tools: List[BaseTool]) -> List[BaseTool]:
        """
        Wrap tools with AgentAuth authentication.

        This wraps each tool so that any HTTP requests it makes
        include AgentAuth authentication headers.

        Args:
            tools: List of LangChain tools to wrap

        Returns:
            List of wrapped tools with authentication
        """
        return [self._wrap_tool(tool) for tool in tools]

    def _wrap_tool(self, tool: BaseTool) -> BaseTool:
        """Wrap a single tool with authentication."""
        original_run = tool._run
        original_arun = tool._arun
        toolkit = self

        @functools.wraps(original_run)
        def wrapped_run(*args, **kwargs):
            # For sync tools, we need to run the async auth in a new event loop
            loop = asyncio.new_event_loop()
            try:
                headers = loop.run_until_complete(
                    toolkit.get_auth_headers("POST", "https://api.example.com")
                )
                # Inject headers into kwargs if the tool accepts them
                if "headers" in kwargs:
                    kwargs["headers"].update(headers)
                else:
                    kwargs["headers"] = headers
            finally:
                loop.close()
            return original_run(*args, **kwargs)

        async def wrapped_arun(*args, **kwargs):
            headers = await toolkit.get_auth_headers("POST", "https://api.example.com")
            # Inject headers into kwargs if the tool accepts them
            if "headers" in kwargs:
                kwargs["headers"].update(headers)
            else:
                kwargs["headers"] = headers
            return await original_arun(*args, **kwargs)

        # Create a new tool with wrapped methods
        tool._run = wrapped_run
        tool._arun = wrapped_arun
        return tool


# For backwards compatibility
__all__ = ["AgentAuthToolkit"]
