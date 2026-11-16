import { useEffect, useRef, useState, useSyncExternalStore } from "react";
import type { ReactNode } from "react";
import { Button, Input, Select } from "@ghostline/ui-core";
export interface Choice {
  value: string;
  label: string;
}
export interface DialogField {
  name: string;
  label: string;
  value?: string;
  type?: "text" | "textarea" | "number" | "select";
  options?: Choice[];
}
interface DialogRequest {
  title: string;
  message?: string;
  fields?: DialogField[];
  content?: ReactNode;
  confirm?: string;
}
let current:
  | (DialogRequest & {
      resolve: (value: Record<string, string> | null) => void;
    })
  | null = null;
const listeners = new Set<() => void>();
export function ask(
  request: DialogRequest,
): Promise<Record<string, string> | null> {
  if (current) return Promise.reject(Error("Finish the open dialog first."));
  return new Promise((resolve) => {
    current = { ...request, resolve };
    listeners.forEach((fn) => fn());
  });
}
function finish(value: Record<string, string> | null): void {
  const request = current;
  current = null;
  listeners.forEach((fn) => fn());
  request?.resolve(value);
}
export function DialogHost() {
  const request = useSyncExternalStore(
    (fn) => {
      listeners.add(fn);
      return () => listeners.delete(fn);
    },
    () => current,
  );
  return request ? <Dialog key={request.title} request={request} /> : null;
}
function Dialog({ request }: { request: DialogRequest }) {
  const ref = useRef<HTMLDialogElement>(null);
  const [values, setValues] = useState<Record<string, string>>(() =>
    Object.fromEntries(
      (request.fields || []).map((f) => [
        f.name,
        f.value ?? f.options?.[0]?.value ?? "",
      ]),
    ),
  );
  useEffect(() => {
    ref.current?.showModal();
  }, []);
  return (
    <dialog
      ref={ref}
      onCancel={() => finish(null)}
      aria-labelledby="dialog-title"
      className="m-auto max-h-[90dvh] w-[min(680px,95vw)] overflow-auto rounded-xl border border-line bg-panel p-6 text-foreground shadow-2xl backdrop:bg-black/70"
    >
      <form
        onSubmit={(e) => {
          e.preventDefault();
          finish(values);
        }}
        className="space-y-5"
      >
        <h2 id="dialog-title" className="text-xl font-semibold">
          {request.title}
        </h2>
        {request.message && <p className="text-muted">{request.message}</p>}
        {request.content}
        {request.fields?.map((field, i) => (
          <label key={field.name} className="grid gap-2 text-sm">
            {field.label}
            {field.options ? (
              <Select
                autoFocus={i === 0}
                value={values[field.name]}
                onChange={(e) =>
                  setValues({ ...values, [field.name]: e.target.value })
                }
              >
                {field.options.map((o) => (
                  <option key={o.value} value={o.value}>
                    {o.label}
                  </option>
                ))}
              </Select>
            ) : field.type === "textarea" ? (
              <textarea
                autoFocus={i === 0}
                className="min-h-32 rounded border border-line bg-background p-3"
                value={values[field.name]}
                onChange={(e) =>
                  setValues({ ...values, [field.name]: e.target.value })
                }
              />
            ) : (
              <Input
                autoFocus={i === 0}
                type={field.type || "text"}
                value={values[field.name]}
                onChange={(e) =>
                  setValues({ ...values, [field.name]: e.target.value })
                }
              />
            )}
          </label>
        ))}
        <div className="flex justify-end gap-3">
          <Button type="button" onClick={() => finish(null)}>
            Cancel
          </Button>
          <Button variant="primary" type="submit">
            {request.confirm || "Continue"}
          </Button>
        </div>
      </form>
    </dialog>
  );
}
