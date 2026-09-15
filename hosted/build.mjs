import { execFileSync } from "node:child_process";
import { chmodSync, copyFileSync, mkdirSync, rmSync } from "node:fs";
import { homedir, tmpdir } from "node:os";
import { join } from "node:path";

const wanted = "1.98.0";
const cache = process.env.VERCEL ? join(process.env.XDG_CACHE_HOME ?? join(homedir(), ".cache"), "pasaporte-rust") : join(tmpdir(), "pasaporte-rust");
const rustupHome = join(cache, "rustup");
const cargoHome = join(cache, "cargo");
let env = { ...process.env };
let cargo = "cargo";
try {
  const version = execFileSync("rustc", ["--version"], { encoding: "utf8" });
  if (!version.startsWith(`rustc ${wanted} `)) throw new Error("wrong version");
} catch {
  env = { ...process.env, RUSTUP_HOME: rustupHome, CARGO_HOME: cargoHome };
  mkdirSync(cache, { recursive: true });
  const installer = join(cache, "rustup-init.sh");
  execFileSync("curl", ["--proto", "=https", "--tlsv1.2", "-fsS", "https://sh.rustup.rs", "-o", installer], { stdio: "inherit" });
  execFileSync("sh", [installer, "-y", "--no-modify-path", "--profile", "minimal", "--default-toolchain", wanted], { env, stdio: "inherit" });
  cargo = join(cargoHome, "bin", "cargo");
}
execFileSync(cargo, ["build", "--release", "--locked"], { env, stdio: "inherit" });
mkdirSync("hosted/bin", { recursive: true });
rmSync("hosted/bin/pasaporte-cafe-mcp", { force: true });
copyFileSync("target/release/pasaporte-cafe-mcp", "hosted/bin/pasaporte-cafe-mcp");
chmodSync("hosted/bin/pasaporte-cafe-mcp", 0o755);
