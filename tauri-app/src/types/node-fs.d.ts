declare module 'node:fs' {
  export function readdirSync(path: string | URL): string[];
  export function readFileSync(path: string | URL, encoding: string): string;
  export function statSync(path: string | URL): { isDirectory(): boolean };
}

declare const process: {
  cwd(): string;
};
