import { MAIL_PREFIX } from "./contracts";
export function mailApiMethods(pathname:string):readonly string[]|null {
  if(!pathname.startsWith(`${MAIL_PREFIX}/`)) return null;
  const path=pathname.slice(MAIL_PREFIX.length);
  if(path==="/connections") return ["GET"];
  if(/^\/connections\/conn_[0-9a-f-]{36}$/.test(path)) return ["GET","DELETE"];
  if(/^\/connections\/conn_[0-9a-f-]{36}\/access-token$/.test(path)) return ["POST"];
  return null;
}
