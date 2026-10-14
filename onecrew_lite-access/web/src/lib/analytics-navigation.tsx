import { useEffect } from "react";
import { useLocation } from "react-router";

declare global {
  interface Window {
    onecrewAnalytics?: {
      surface: "marketing" | "access";
      pageView(key: string): void;
      business(name: "sign_up" | "generate_lead"): void;
      ready(): Promise<boolean>;
    };
  }
}

export function AnalyticsNavigation() {
  const location = useLocation();
  useEffect(() => {
    try { window.onecrewAnalytics?.pageView(location.key); } catch { /* Analytics never blocks routing. */ }
  }, [location.key]);
  return null;
}
