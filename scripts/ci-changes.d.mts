// Types for ci-changes.mjs, which stays plain JavaScript so CI can run it before `npm ci`.

export function isMarkdownOnly(files: string[]): boolean;
export function changedFiles(base: string, cwd?: string): string[];
export function needsBuild(options: { event?: string; base?: string; cwd?: string }): boolean;
