"""
Tests for AgentAuth Python bindings.
"""

import pytest


def test_import_capability():
    """Test that Capability can be imported."""
    from agentauth import Capability

    # Create different capability types
    read_cap = Capability.read("calendar")
    write_cap = Capability.write("calendar")
    transact_cap = Capability.transact("payments", 1000)
    delete_cap = Capability.delete("documents")
    custom_cap = Capability.custom("myns", "myaction", {"key": "value"})

    # Verify they have repr
    assert "Read" in repr(read_cap)
    assert "Write" in repr(write_cap)
    assert "Transact" in repr(transact_cap)
    assert "Delete" in repr(delete_cap)
    assert "Custom" in repr(custom_cap)


def test_import_behavioral_envelope():
    """Test that BehavioralEnvelope can be imported."""
    from agentauth import BehavioralEnvelope

    # Create with defaults
    envelope = BehavioralEnvelope()
    assert "30 actions per minute" in envelope.to_human_readable()

    # Create restrictive
    restrictive = BehavioralEnvelope.default_restrictive()
    assert "online" in restrictive.to_human_readable()

    # Create permissive
    permissive = BehavioralEnvelope.default_permissive()
    assert "600 actions per minute" in permissive.to_human_readable()


def test_import_version():
    """Test that version is available."""
    from agentauth import __version__

    assert __version__ is not None
    assert len(__version__) > 0


def test_capability_with_filter():
    """Test capability with filter."""
    from agentauth import Capability

    cap = Capability.read("emails", filter="unread")
    assert "Read" in repr(cap)


def test_capability_with_conditions():
    """Test capability with conditions."""
    from agentauth import Capability, WriteConditions

    conditions = WriteConditions(filter="owner:self", append_only=True)
    cap = Capability.write("calendar", conditions=conditions)
    assert "Write" in repr(cap)


def test_behavioral_envelope_custom():
    """Test custom behavioral envelope."""
    from agentauth import BehavioralEnvelope

    envelope = BehavioralEnvelope(
        max_requests_per_minute=100,
        max_burst=20,
        requires_human_online=True,
        human_confirmation_threshold=500,
        max_session_duration_secs=1800,
    )

    readable = envelope.to_human_readable()
    assert "100 actions per minute" in readable
    assert "online" in readable
    assert "500" in readable
