// Готовит окружение и запускает сборку или dev-сервер фронтенда.
//
// Зачем отдельный скрипт: esbuild пишет промежуточный файл в системный TEMP
// и затем не может его удалить (Access is denied в песочнице), из-за чего
// `vite build` падает с ошибкой. Поэтому TMP и TEMP перенаправляются внутрь
// проекта, а команда запускается дочерним процессом с этим окружением.

import { spawnSync } from "node:child_process";
import { mkdirSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const tmp = join(root, "node_modules", ".tmp-build");
mkdirSync(tmp, { recursive: true });

const env = { ...process.env, TMP: tmp, TEMP: tmp };
const mode = process.argv[2] ?? "build";

const steps =
  mode === "dev"
    ? [["npx", ["vite"]]]
    : [
        ["npx", ["tsc", "-b"]],
        ["npx", ["vite", "build"]],
      ];

for (const [cmd, args] of steps) {
  const result = spawnSync(cmd, args, {
    cwd: root,
    env,
    stdio: "inherit",
    shell: process.platform === "win32",
  });
  if (result.status !== 0) {
    process.exit(result.status ?? 1);
  }
}
