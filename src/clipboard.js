import { spawn } from "node:child_process";

export function readClipboard() {
  return new Promise((resolve, reject) => {
    const proc = spawn("pbpaste", [], { stdio: ["ignore", "pipe", "pipe"] });
    let out = "";
    let err = "";
    proc.stdout.on("data", (c) => (out += c.toString("utf8")));
    proc.stderr.on("data", (c) => (err += c.toString("utf8")));
    proc.on("error", reject);
    proc.on("close", (code) => {
      if (code !== 0) return reject(new Error(`pbpaste exited ${code}: ${err}`));
      resolve(out);
    });
  });
}

export function writeClipboard(text) {
  return new Promise((resolve, reject) => {
    const proc = spawn("pbcopy", [], { stdio: ["pipe", "ignore", "pipe"] });
    let err = "";
    proc.stderr.on("data", (c) => (err += c.toString("utf8")));
    proc.on("error", reject);
    proc.on("close", (code) => {
      if (code !== 0) return reject(new Error(`pbcopy exited ${code}: ${err}`));
      resolve();
    });
    proc.stdin.end(text, "utf8");
  });
}
