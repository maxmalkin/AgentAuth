import { useState } from 'react';
import { Link } from '../Router';

type DashboardTab = 'token-verification-slo' | 'circuit-breakers';

const GRAFANA_BASE_URL = 'http://localhost:3000';

const DASHBOARDS: Record<DashboardTab, { label: string; uid: string }> = {
  'token-verification-slo': {
    label: 'TOKEN VERIFICATION SLO',
    uid: 'token-verification-slo',
  },
  'circuit-breakers': {
    label: 'CIRCUIT BREAKERS',
    uid: 'circuit-breakers',
  },
};

function getDashboardUrl(uid: string): string {
  return `${GRAFANA_BASE_URL}/d/${uid}?orgId=1&kiosk`;
}

export function DashboardPage() {
  const [activeTab, setActiveTab] = useState<DashboardTab>('token-verification-slo');
  const [iframeError, setIframeError] = useState(false);

  const activeDashboard = DASHBOARDS[activeTab];
  const iframeSrc = getDashboardUrl(activeDashboard.uid);

  function handleTabChange(tab: DashboardTab) {
    setActiveTab(tab);
    setIframeError(false);
  }

  return (
    <div className="h-screen flex flex-col">
      {/* Top bar */}
      <div className="border-b border-border bg-panel shrink-0">
        <div className="max-w-full mx-auto px-4 sm:px-6 h-12 flex items-center justify-between">
          <Link to="/" className="flex items-center gap-2 text-text-secondary hover:text-amber transition-colors">
            <div className="w-4 h-4 border border-current flex items-center justify-center">
              <div className="w-1.5 h-1.5 bg-current" />
            </div>
            <span className="font-mono text-xs tracking-wide">AGENTAUTH</span>
          </Link>
          <span className="font-mono text-xs text-amber tracking-wide">DASHBOARD</span>
        </div>
      </div>

      {/* Tab bar */}
      <div className="border-b border-border bg-panel shrink-0">
        <div className="max-w-full mx-auto px-4 sm:px-6 flex gap-1">
          {(Object.entries(DASHBOARDS) as [DashboardTab, { label: string; uid: string }][]).map(
            ([key, dashboard]) => (
              <button
                key={key}
                onClick={() => handleTabChange(key)}
                className={`px-4 py-2.5 font-mono text-xs tracking-wide transition-colors border-b-2 ${
                  activeTab === key
                    ? 'border-amber text-amber'
                    : 'border-transparent text-text-secondary hover:text-text-primary hover:border-border-bright'
                }`}
              >
                {dashboard.label}
              </button>
            ),
          )}
        </div>
      </div>

      {/* Dashboard iframe */}
      <div className="flex-1 relative">
        {iframeError ? (
          <div className="absolute inset-0 flex items-center justify-center">
            <div className="max-w-md border border-amber-dim bg-amber-glow p-6">
              <div className="flex items-start gap-3">
                <div className="w-2 h-2 mt-1.5 bg-amber animate-pulse" />
                <div>
                  <h2 className="font-mono text-sm font-medium tracking-wide text-text-primary mb-2">
                    GRAFANA UNREACHABLE
                  </h2>
                  <p className="text-text-secondary text-sm">
                    Unable to load the Grafana dashboard. Verify that Grafana is running at{' '}
                    <span className="font-mono text-amber">{GRAFANA_BASE_URL}</span>.
                  </p>
                </div>
              </div>
              <button
                onClick={() => setIframeError(false)}
                className="mt-5 w-full py-2.5 border border-border text-text-secondary font-mono text-xs tracking-wide hover:border-amber hover:text-amber transition-colors"
              >
                RETRY
              </button>
            </div>
          </div>
        ) : (
          <iframe
            key={activeTab}
            src={iframeSrc}
            title={activeDashboard.label}
            className="w-full h-full border-0"
            onError={() => setIframeError(true)}
          />
        )}
      </div>
    </div>
  );
}
