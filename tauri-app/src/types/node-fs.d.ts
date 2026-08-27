declare module 'node:fs' {
  export function readdirSync(path: string | URL): string[];
  export function readFileSync(path: string | URL, encoding: string): string;
}

declare const process: {
  cwd(): string;
};
