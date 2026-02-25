// Simple client-side router for the approval UI

import { useState, useEffect, createContext, useContext } from 'react';

interface RouteParams {
  [key: string]: string;
}

interface RouterContextValue {
  path: string;
  params: RouteParams;
  navigate: (path: string) => void;
}

const RouterContext = createContext<RouterContextValue>({
  path: '/',
  params: {},
  navigate: () => {},
});

/** Hook to access router context */
export function useRouter() {
  return useContext(RouterContext);
}

/** Hook to get route parameters */
export function useParams<T extends RouteParams>(): T {
  const { params } = useRouter();
  return params as T;
}

/** Parse route pattern and extract params */
function matchRoute(
  pattern: string,
  path: string
): RouteParams | null {
  const patternParts = pattern.split('/').filter(Boolean);
  const pathParts = path.split('/').filter(Boolean);

  if (patternParts.length !== pathParts.length) {
    return null;
  }

  const params: RouteParams = {};

  for (let i = 0; i < patternParts.length; i++) {
    const patternPart = patternParts[i]!;
    const pathPart = pathParts[i]!;

    if (patternPart.startsWith(':')) {
      // This is a parameter
      const paramName = patternPart.slice(1);
      params[paramName] = decodeURIComponent(pathPart);
    } else if (patternPart !== pathPart) {
      // Static part doesn't match
      return null;
    }
  }

  return params;
}

interface Route {
  pattern: string;
  component: React.ComponentType;
}

interface RouterProps {
  routes: Route[];
  notFound?: React.ComponentType;
}

/** Router component */
export function Router({ routes, notFound: NotFound }: RouterProps) {
  const [path, setPath] = useState(window.location.pathname);

  useEffect(() => {
    const handlePopState = () => {
      setPath(window.location.pathname);
    };

    window.addEventListener('popstate', handlePopState);
    return () => window.removeEventListener('popstate', handlePopState);
  }, []);

  const navigate = (newPath: string) => {
    window.history.pushState({}, '', newPath);
    setPath(newPath);
  };

  // Find matching route
  let matchedRoute: Route | null = null;
  let params: RouteParams = {};

  for (const route of routes) {
    const match = matchRoute(route.pattern, path);
    if (match !== null) {
      matchedRoute = route;
      params = match;
      break;
    }
  }

  const contextValue: RouterContextValue = {
    path,
    params,
    navigate,
  };

  return (
    <RouterContext.Provider value={contextValue}>
      {matchedRoute ? (
        <matchedRoute.component />
      ) : NotFound ? (
        <NotFound />
      ) : (
        <div className="not-found">
          <h1>404 - Page Not Found</h1>
          <p>The page you're looking for doesn't exist.</p>
        </div>
      )}
    </RouterContext.Provider>
  );
}

/** Link component for navigation */
interface LinkProps extends React.AnchorHTMLAttributes<HTMLAnchorElement> {
  to: string;
  children: React.ReactNode;
}

export function Link({ to, children, onClick, ...props }: LinkProps) {
  const { navigate } = useRouter();

  const handleClick = (e: React.MouseEvent<HTMLAnchorElement>) => {
    e.preventDefault();
    if (onClick) onClick(e);
    navigate(to);
  };

  return (
    <a href={to} onClick={handleClick} {...props}>
      {children}
    </a>
  );
}
