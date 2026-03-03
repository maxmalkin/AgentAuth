import { Router, Link } from './Router';
import { ApprovalPage, AgentsPage, AgentActivityPage, DashboardPage } from './pages';

const routes = [
  { pattern: '/', component: HomePage },
  { pattern: '/approve/:grant_id', component: ApprovalPage },
  { pattern: '/agents', component: AgentsPage },
  { pattern: '/agents/:agent_id/activity', component: AgentActivityPage },
  { pattern: '/dashboard', component: DashboardPage },
];

function HomePage() {
  return (
    <div className="min-h-screen flex flex-col items-center justify-center px-4">
      <div className="text-center animate-fade-in">
        {/* Logo mark */}
        <div className="mb-8 flex items-center justify-center gap-3">
          <div className="w-10 h-10 border border-amber bg-amber-glow flex items-center justify-center">
            <svg width="20" height="20" viewBox="0 0 20 20" fill="none">
              <path d="M10 2L18 6V14L10 18L2 14V6L10 2Z" stroke="currentColor" strokeWidth="1.5" className="text-amber" />
              <circle cx="10" cy="10" r="3" stroke="currentColor" strokeWidth="1.5" className="text-amber" />
            </svg>
          </div>
          <span className="font-mono text-2xl font-medium tracking-tight text-text-primary">
            AGENTAUTH
          </span>
        </div>

        <p className="text-text-secondary text-sm font-mono tracking-wide mb-1">
          PERMISSION CONTROL INTERFACE
        </p>
        <div className="w-48 h-px bg-border mx-auto mb-10" />

        <div className="flex flex-col sm:flex-row gap-3 justify-center">
          <Link
            to="/agents"
            className="inline-flex items-center gap-2 px-6 py-3 bg-amber text-surface font-mono text-sm font-medium tracking-wide hover:bg-amber-dim transition-colors"
          >
            <svg width="16" height="16" viewBox="0 0 16 16" fill="none">
              <rect x="2" y="2" width="5" height="5" stroke="currentColor" strokeWidth="1.5" />
              <rect x="9" y="2" width="5" height="5" stroke="currentColor" strokeWidth="1.5" />
              <rect x="2" y="9" width="5" height="5" stroke="currentColor" strokeWidth="1.5" />
              <rect x="9" y="9" width="5" height="5" stroke="currentColor" strokeWidth="1.5" />
            </svg>
            VIEW AGENTS
          </Link>
          <Link
            to="/dashboard"
            className="inline-flex items-center gap-2 px-6 py-3 border border-border text-text-secondary font-mono text-sm font-medium tracking-wide hover:border-amber hover:text-amber transition-colors"
          >
            <svg width="16" height="16" viewBox="0 0 16 16" fill="none">
              <path d="M2 12L5.5 6L8.5 9L14 3" stroke="currentColor" strokeWidth="1.5" />
              <path d="M2 14H14" stroke="currentColor" strokeWidth="1.5" />
            </svg>
            DASHBOARD
          </Link>
        </div>
      </div>

      {/* Grid decoration */}
      <div className="fixed inset-0 pointer-events-none -z-10 overflow-hidden">
        <div className="absolute top-1/2 left-1/2 -translate-x-1/2 -translate-y-1/2 w-[600px] h-[600px] border border-border/20 rotate-45 opacity-30" />
        <div className="absolute top-1/2 left-1/2 -translate-x-1/2 -translate-y-1/2 w-[400px] h-[400px] border border-border/20 rotate-45 opacity-20" />
        <div className="absolute top-1/2 left-1/2 -translate-x-1/2 -translate-y-1/2 w-[200px] h-[200px] border border-amber/10 rotate-45 opacity-40" />
      </div>
    </div>
  );
}

function NotFound() {
  return (
    <div className="min-h-screen flex flex-col items-center justify-center px-4 text-center">
      <div className="font-mono text-6xl font-bold text-border mb-4">404</div>
      <p className="text-text-secondary mb-6 text-sm font-mono">SECTOR NOT FOUND</p>
      <Link
        to="/"
        className="inline-flex items-center gap-2 px-5 py-2.5 border border-border text-text-secondary font-mono text-xs tracking-wide hover:border-amber hover:text-amber transition-colors"
      >
        RETURN TO BASE
      </Link>
    </div>
  );
}

function App() {
  return (
    <div className="min-h-screen">
      <Router routes={routes} notFound={NotFound} />
    </div>
  );
}

export default App;
