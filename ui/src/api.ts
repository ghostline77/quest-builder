import type {
  Bootstrap,
  Catalog,
  DiskFile,
  FileGroup,
  Job,
  Json,
} from "./model";
interface Endpoints {
  bootstrap: Bootstrap;
  catalog: Catalog;
  files: FileGroup[];
  read: DiskFile;
  save: { hash: string };
  jobs: Job[];
  "jobs/cancel": { cancelled?: boolean };
  history: { versions: string[]; text?: string };
}
export class Client {
  token = "";
  async request<K extends keyof Endpoints>(
    path: K,
    data?: unknown,
  ): Promise<Endpoints[K]> {
    const response = await fetch("/api/" + path, {
      method: data === undefined ? "GET" : "POST",
      headers: {
        "content-type": "application/json",
        "x-quest-token": this.token,
      },
      ...(data === undefined ? {} : { body: JSON.stringify(data) }),
    });
    const value = (await response.json()) as Endpoints[K] & { error?: string };
    if (!response.ok)
      throw Error(value.error || `${response.status} ${response.statusText}`);
    return value;
  }
  async start(root: number, command: string, args: string[]): Promise<string> {
    const response = await fetch("/api/jobs", {
      method: "POST",
      headers: {
        "content-type": "application/json",
        "x-quest-token": this.token,
      },
      body: JSON.stringify({ root, command, args }),
    });
    const value = (await response.json()) as { id: string; error?: string };
    if (!response.ok) throw Error(value.error || response.statusText);
    return value.id;
  }
  async media(root: number, path: string): Promise<string> {
    const response = await fetch("/api/media", {
      method: "POST",
      headers: {
        "content-type": "application/json",
        "x-quest-token": this.token,
      },
      body: JSON.stringify({ root, path }),
    });
    if (!response.ok)
      throw Error(((await response.json()) as { error: string }).error);
    return URL.createObjectURL(await response.blob());
  }
}
export const client = new Client();
export const saveNew = (root: number, path: string, value: Json) =>
  client.request("save", {
    root,
    path,
    hash: null,
    text: JSON.stringify(value, null, 2) + "\n",
  });
