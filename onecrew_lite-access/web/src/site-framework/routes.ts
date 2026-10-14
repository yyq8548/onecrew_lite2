import { type RouteConfig, index, route } from "@react-router/dev/routes";

export default [
  index("./site-home-route.tsx"),
  route("*", "./site-route.tsx", { id: "localized-site-route" }),
] satisfies RouteConfig;
