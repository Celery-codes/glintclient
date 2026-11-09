import { openUrl } from "@tauri-apps/plugin-opener";
import { Check, Copy, ExternalLink, Loader2 } from "lucide-react";
import { useState } from "react";
import { useNavigate } from "react-router-dom";
import { useAccounts } from "../stores/accounts";
import { Button, Dialog } from "./ui";

/** Device-code sign-in: show a code, the user enters it at microsoft.com/link. */
export function SignInDialog() {
  const login = useAccounts((s) => s.login);
  const cancel = useAccounts((s) => s.cancelLogin);
  const retry = useAccounts((s) => s.beginLogin);
  const afterLogin = useAccounts((s) => s.afterLogin);
  const [copied, setCopied] = useState(false);
  const navigate = useNavigate();

  if (!login.open) return null;
  const { prompt, error } = login;

  const copyAndOpen = async () => {
    if (!prompt) return;
    await navigator.clipboard.writeText(prompt.user_code).catch(() => {});
    setCopied(true);
    await openUrl(prompt.verification_uri);
  };

  return (
    <Dialog title="Sign in with Microsoft" onClose={cancel}>
      {login.starting && (
        <div className="flex items-center gap-3 py-6 text-muted">
          <Loader2 size={18} className="animate-spin" />
          Contacting Microsoft…
        </div>
      )}

      {prompt && (
        <div className="flex flex-col gap-4 pb-2">
          <p className="text-muted">
            Enter this code on Microsoft's sign-in page, using the account that owns Minecraft.
          </p>
          <div className="selectable rounded-md border border-border bg-bg py-4 text-center font-mono text-[28px] font-semibold tracking-[0.2em] text-accent">
            {prompt.user_code}
          </div>
          <Button
            variant="primary"
            onClick={copyAndOpen}
            icon={copied ? <Check size={16} /> : <Copy size={16} />}
          >
            {copied ? "Code copied — open the page again" : "Copy code and open sign-in page"}
          </Button>
          <div className="flex items-center gap-2 text-[13px] text-muted">
            <Loader2 size={14} className="animate-spin" />
            Waiting for you to finish in the browser…
          </div>
          <p className="selectable flex items-center gap-1.5 text-[12.5px] text-faint">
            <ExternalLink size={12} />
            {prompt.verification_uri}
          </p>
        </div>
      )}

      {error && (
        <div className="flex flex-col gap-4 pb-2">
          <p className="selectable text-[13.5px] leading-relaxed">{error.message}</p>
          <div className="flex justify-end gap-2">
            {error.kind === "auth_missing_client_id" ? (
              <Button
                variant="primary"
                onClick={() => {
                  cancel();
                  navigate("/settings");
                }}
              >
                Open Settings
              </Button>
            ) : (
              <Button variant="primary" onClick={() => void retry(afterLogin ?? undefined)}>
                Try again
              </Button>
            )}
          </div>
        </div>
      )}
    </Dialog>
  );
}
