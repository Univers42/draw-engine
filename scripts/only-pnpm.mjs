const execPath = process.env.npm_execpath ?? "";
if (!execPath.includes("pnpm")) {
  console.error("Use pnpm. npm and yarn are not supported.");
  process.exit(1);
}
