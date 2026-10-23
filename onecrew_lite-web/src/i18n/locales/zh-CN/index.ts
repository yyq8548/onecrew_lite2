import accounts from "./accounts";
import common from "./common";
import connect from "./connect";
import inbox from "./inbox";
import leads from "./leads";
import mail from "./mail";
import pairing from "./pairing";
import shell from "./shell";
import sidebar from "./sidebar";
import updates from "./updates";

const zhCN = {
  accounts,
  common,
  connect,
  inbox,
  leads,
  mail,
  pairing,
  shell,
  sidebar,
  updates,
} as const;

export default zhCN;
