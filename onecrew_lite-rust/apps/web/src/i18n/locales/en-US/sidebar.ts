import type { CatalogShape } from "../../catalog-shape";
import type zhCNSidebar from "../zh-CN/sidebar";

const sidebar = {
  toggle: {
    expand: "Expand sidebar",
    collapse: "Collapse sidebar",
  },
  sections: {
    discover: "Discover & manage",
    communicate: "Communicate & engage",
    accounts: "Accounts & connections",
    free: "onecrew_lite",
  },
  navigation: {
    free: "Connections & login",
    freeSearch: "Search",
    leads: "Leads",
    inbox: "Messages",
    mail: "Email",
    accounts: "Settings",
    connect: "Connect",
  },
  language: {
    chinese: "简体中文",
    english: "English",
    switchToChinese: "Switch to 简体中文",
    switchToEnglish: "Switch to English",
  },
} as const satisfies CatalogShape<typeof zhCNSidebar>;

export default sidebar;
