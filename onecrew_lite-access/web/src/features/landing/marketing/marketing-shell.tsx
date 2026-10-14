import { Outlet } from "react-router";

export function MarketingShell() {
  return (
    <div className="landing-page">
      <Outlet />
    </div>
  );
}
