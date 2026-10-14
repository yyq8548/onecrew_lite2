import { createHash } from "node:crypto";
import { readFile } from "node:fs/promises";

const root = new URL("../", import.meta.url);
const frozenMigrations = new Map([
  [
    "0001_better_auth.sql",
    "d83fcddd09a8a48d773614fbe005b05a08bbd6b1a231abe39bab4b57b87da1c7",
  ],
  [
    "0002_activation_devices.sql",
    "3a65e117336bec6d8ed9ae492387bdd6f714b4a65fadb8556efe9d50b9a687dd",
  ],
  [
    "0003_desktop_pairing_grants.sql",
    "9359c2b16852a48f4f900731f3a78b379f1e4f274cb9286534a1e389483389e2",
  ],
  [
    "0004_registration_ip.sql",
    "e2e2467652ccfcb0b4f1da18e895ebe0299abfb878a37f8f571454cbb91a66c3",
  ],
  [
    "0005_device_pairing_ip.sql",
    "cac469be291c923cf0284aedb7857b7ba63397042592b6bc2b27e19374c80d6d",
  ],
  [
    "0006_login_risk_session_idx.sql",
    "f1fe3137c056a7895c0d61a9089cabb33e25a86b2d08a20c3ab6b93cf801fa29",
  ],
  [
    "0007_third_party_oauth.sql",
    "eb0a0602e7062469a48c790daabd06a416d38bd7fffaef377dbd866108696778",
  ],
]);

let valid = true;
for (const [name, expectedDigest] of frozenMigrations) {
  const contents = await readFile(new URL(`migrations/${name}`, root));
  const actualDigest = createHash("sha256").update(contents).digest("hex");
  if (actualDigest !== expectedDigest) {
    valid = false;
    console.error(`${name} changed after deployment; append a new migration instead.`);
  }
}

if (!valid) process.exitCode = 1;
