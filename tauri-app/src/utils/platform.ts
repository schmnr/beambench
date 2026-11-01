export function isMacPlatform(navigatorLike: Pick<Navigator, 'platform' | 'userAgent'> = navigator): boolean {
  const platform = navigatorLike.platform.toLowerCase();
  const userAgent = navigatorLike.userAgent.toLowerCase();
  return platform.includes('mac') || userAgent.includes('mac os x');
}

export function isNativeMenuActive(): boolean {
  return isMacPlatform();
}

export function isLinuxPlatform(navigatorLike: Pick<Navigator, 'platform' | 'userAgent'> = navigator): boolean {
  return navigatorLike.platform.toLowerCase().includes('linux');
}
