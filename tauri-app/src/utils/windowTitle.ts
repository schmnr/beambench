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
  port?: string | null,
  profileName?: string | null,
): string {
  const visibleName = projectDisplayName(projectPath, projectName);
  const project = visibleName ? `${visibleName}${dirty ? ' *' : ''} - ${APP_NAME}` : APP_NAME;
  // Keep the endpoint visible even when the taskbar truncates a long project name.
  const title = port ? [port, profileName, project].filter(Boolean).join(' · ') : project;
  return Array.from(title).slice(0, 512).join('');
}
