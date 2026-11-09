import { create } from "zustand";
import {
  ipc,
  toAppError,
  type Account,
  type Accounts,
  type AppError,
  type AuthFinishedEvent,
  type LoginPrompt,
} from "../lib/ipc";
import { toast } from "./toasts";

interface LoginState {
  open: boolean;
  /** Waiting for Microsoft to hand back a code. */
  starting: boolean;
  prompt: LoginPrompt | null;
  error: AppError | null;
}

const LOGIN_CLOSED: LoginState = { open: false, starting: false, prompt: null, error: null };

interface AccountStore {
  accounts: Accounts;
  login: LoginState;
  /** Runs once sign-in succeeds, e.g. the launch that asked for it. */
  afterLogin: (() => void) | null;
  load: () => Promise<void>;
  active: () => Account | null;
  beginLogin: (afterLogin?: () => void) => Promise<void>;
  cancelLogin: () => void;
  finishLogin: (event: AuthFinishedEvent) => void;
  setActive: (uuid: string) => Promise<void>;
  remove: (uuid: string) => Promise<void>;
}

export const useAccounts = create<AccountStore>((set, get) => ({
  accounts: { active: null, accounts: [] },
  login: LOGIN_CLOSED,
  afterLogin: null,

  load: async () => set({ accounts: await ipc.listAccounts() }),

  active: () => {
    const { active, accounts } = get().accounts;
    return accounts.find((a) => a.uuid === active) ?? null;
  },

  beginLogin: async (afterLogin) => {
    set({
      login: { open: true, starting: true, prompt: null, error: null },
      afterLogin: afterLogin ?? null,
    });
    try {
      const prompt = await ipc.authBegin();
      // The dialog may have been closed while the request was in flight.
      if (get().login.open) {
        set({ login: { open: true, starting: false, prompt, error: null } });
      } else {
        void ipc.authCancel();
      }
    } catch (error) {
      set({ login: { open: true, starting: false, prompt: null, error: toAppError(error) } });
    }
  },

  cancelLogin: () => {
    void ipc.authCancel();
    set({ login: LOGIN_CLOSED, afterLogin: null });
  },

  finishLogin: (event) => {
    if (event.account) {
      const afterLogin = get().afterLogin;
      set({ login: LOGIN_CLOSED, afterLogin: null });
      toast.success(`Signed in as ${event.account.username}`);
      void get().load().then(() => afterLogin?.());
    } else if (get().login.open) {
      set({
        login: { open: true, starting: false, prompt: null, error: event.error },
      });
    }
  },

  setActive: async (uuid) => set({ accounts: await ipc.setActiveAccount(uuid) }),

  remove: async (uuid) => set({ accounts: await ipc.removeAccount(uuid) }),
}));
