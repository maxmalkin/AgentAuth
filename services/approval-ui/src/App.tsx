import { Router, Link } from './Router';
import { ApprovalPage, AgentsPage, AgentActivityPage } from './pages';

const routes = [
  { pattern: '/', component: HomePage },
  { pattern: '/approve/:grant_id', component: ApprovalPage },
  { pattern: '/agents', component: AgentsPage },
  { pattern: '/agents/:agent_id/activity', component: AgentActivityPage },
];

function HomePage() {
  return (
    <div className="page home-page">
      <header className="page-header">
        <h1>AgentAuth</h1>
        <p>Manage your AI agent permissions</p>
      </header>
      <nav className="home-nav">
        <Link to="/agents" className="btn btn-primary btn-large">
          View Your Agents
        </Link>
      </nav>
    </div>
  );
}

function NotFound() {
  return (
    <div className="page not-found-page">
      <h1>404 - Page Not Found</h1>
      <p>The page you're looking for doesn't exist.</p>
      <Link to="/" className="btn btn-primary">
        Go Home
      </Link>
    </div>
  );
}

function App() {
  return (
    <div className="app">
      <Router routes={routes} notFound={NotFound} />
    </div>
  );
}

export default App;
