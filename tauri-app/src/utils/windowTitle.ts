const APP_NAME = 'Beam Bench';

function basename(path: string): string {
  const parts = path.split(/[\\/]/).filter(Boolean);
  return parts[parts.length - 1] ?? path;
}

export function projectDisplayName(
  projectPath: string | null,
  projectName: string | null,
): string | null {
  return projectPath ? basename(projectPath) : projectName?.trim() || null;
}

export function projectWindowTitle(
  projectPath: string | null,
  projectName: string | null,
  dirty: boolean,
): string {
  const visibleName = projectDisplayName(projectPath, projectName);
  if (!visibleName) return APP_NAME;
  return `${visibleName}${dirty ? ' *' : ''} - ${APP_NAME}`;
}
