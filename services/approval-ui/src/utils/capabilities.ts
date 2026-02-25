// Capability translation utilities - converts capabilities to human-readable text

import type { Capability, BehavioralEnvelope, TimeWindow } from '../types';

/** Check if a capability requires two-step confirmation */
export function requiresTwoStep(capability: Capability): boolean {
  return capability.type === 'Transact' || capability.type === 'Delete';
}

/** Get the risk level of a capability */
export function getCapabilityRiskLevel(
  capability: Capability
): 'low' | 'medium' | 'high' {
  switch (capability.type) {
    case 'Read':
      return 'low';
    case 'Write':
      return 'medium';
    case 'Transact':
    case 'Delete':
      return 'high';
    case 'Custom':
      // Custom capabilities are medium risk by default
      return 'medium';
  }
}

/** Translate a capability to human-readable text */
export function capabilityToHumanReadable(capability: Capability): string {
  switch (capability.type) {
    case 'Read':
      if (capability.filter) {
        return `Read ${capability.resource} (filtered: ${capability.filter})`;
      }
      return `Read ${capability.resource}`;

    case 'Write':
      if (capability.conditions && Object.keys(capability.conditions).length > 0) {
        const condStr = Object.entries(capability.conditions)
          .map(([k, v]) => `${k}=${v}`)
          .join(', ');
        return `Write to ${capability.resource} (conditions: ${condStr})`;
      }
      return `Write to ${capability.resource}`;

    case 'Transact':
      return `Make transactions on ${capability.resource} up to ${formatCurrency(capability.max_value)}`;

    case 'Delete':
      if (capability.filter) {
        return `Delete from ${capability.resource} (filtered: ${capability.filter})`;
      }
      return `Delete from ${capability.resource}`;

    case 'Custom':
      const params = Object.entries(capability.params)
        .map(([k, v]) => `${k}=${v}`)
        .join(', ');
      return `${capability.namespace}:${capability.name}${params ? ` (${params})` : ''}`;
  }
}

/** Format currency value */
function formatCurrency(value: number): string {
  return new Intl.NumberFormat('en-US', {
    style: 'currency',
    currency: 'USD',
  }).format(value);
}

/** Translate behavioral envelope to human-readable text */
export function envelopeToHumanReadable(envelope: BehavioralEnvelope): string[] {
  const descriptions: string[] = [];

  // Rate limits
  descriptions.push(
    `Up to ${envelope.max_requests_per_minute} actions per minute`
  );

  if (envelope.max_burst > 1) {
    descriptions.push(`Can burst up to ${envelope.max_burst} actions at once`);
  }

  // Session duration
  const hours = Math.floor(envelope.max_session_duration_secs / 3600);
  const minutes = Math.floor(
    (envelope.max_session_duration_secs % 3600) / 60
  );
  if (hours > 0) {
    descriptions.push(
      `Sessions last up to ${hours} hour${hours > 1 ? 's' : ''}${minutes > 0 ? ` ${minutes} minutes` : ''}`
    );
  } else {
    descriptions.push(`Sessions last up to ${minutes} minutes`);
  }

  // Human online requirement
  if (envelope.requires_human_online) {
    descriptions.push('Requires you to be online while active');
  }

  // Confirmation threshold
  if (envelope.human_confirmation_threshold !== null) {
    descriptions.push(
      `Will ask for confirmation for transactions over ${formatCurrency(envelope.human_confirmation_threshold)}`
    );
  }

  // Time windows
  if (envelope.allowed_time_windows && envelope.allowed_time_windows.length > 0) {
    const windowDescs = envelope.allowed_time_windows.map(timeWindowToString);
    descriptions.push(`Only active during: ${windowDescs.join(', ')}`);
  }

  return descriptions;
}

/** Format a time window to human-readable string */
function timeWindowToString(window: TimeWindow): string {
  const startTime = formatTime(window.start_hour, window.start_minute);
  const endTime = formatTime(window.end_hour, window.end_minute);
  const days = window.days_of_week.map(dayName).join(', ');

  if (window.days_of_week.length === 7) {
    return `${startTime} - ${endTime} every day`;
  }
  return `${startTime} - ${endTime} on ${days}`;
}

/** Format time as HH:MM AM/PM */
function formatTime(hour: number, minute: number): string {
  const h = hour % 12 || 12;
  const ampm = hour < 12 ? 'AM' : 'PM';
  const m = minute.toString().padStart(2, '0');
  return `${h}:${m} ${ampm}`;
}

/** Get day name from day number (0 = Sunday) */
function dayName(day: number): string {
  const days = ['Sun', 'Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat'];
  return days[day] || 'Unknown';
}

/** Get a summary of all capabilities */
export function getCapabilitySummary(capabilities: Capability[]): {
  total: number;
  byType: Record<string, number>;
  hasHighRisk: boolean;
} {
  const byType: Record<string, number> = {};
  let hasHighRisk = false;

  for (const cap of capabilities) {
    byType[cap.type] = (byType[cap.type] || 0) + 1;
    if (getCapabilityRiskLevel(cap) === 'high') {
      hasHighRisk = true;
    }
  }

  return {
    total: capabilities.length,
    byType,
    hasHighRisk,
  };
}
