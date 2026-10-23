import { acquireInstanceLock } from "../../src/service/instance-lock.ts";

const directory = process.argv[2];
if (!directory || !process.send) throw new Error("The owner test requires a directory and IPC channel");
const lease = await acquireInstanceLock(directory);
process.send({ state: "owned" });
process.on("message", async () => {
  await lease.close();
  if (!process.disconnect) throw new Error("Owner IPC channel is unavailable");
  process.disconnect();
});
