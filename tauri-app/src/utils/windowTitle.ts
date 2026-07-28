const APP_NAME = 'Beam Bench';

function basename(path: string): string {
  const parts = path.split(/[\\/]/).filter(Boolean);
  return parts[parts.length - 1] ?? path;
}

export function projectWindowTitle(
  projectPath: string | null,
  projectName: string | null,
  dirty: boolean,
): string {
  const visibleName = projectPath ? basename(projectPath) : projectName?.trim();
  if (!visibleName) return APP_NAME;
  return `${visibleName}${dirty ? ' *' : ''} - ${APP_NAME}`;
}
