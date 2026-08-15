// SPDX-License-Identifier: MPL-2.0

import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState } from "react";
import { tauriApi } from "./tauriApi.js";
import { LAST_REPOSITORY_KEY, selectDefaultTarget } from "./verificationPhases.js";

const VerificationWorkspaceContext = createContext(null);
const TERMINAL_SESSION_STATES = new Set(["blocked", "verified", "started_unverified", "cancelled", "internal_error"]);

function savedRepository() {
  try {
    return globalThis.localStorage?.getItem(LAST_REPOSITORY_KEY) || "";
  } catch {
    return "";
  }
}

export function VerificationWorkspaceProvider({ children, t, onReceiptChanged }) {
  const [repositoryRoot, setRepositoryRoot] = useState(savedRepository);
  const [plan, setPlan] = useState(null);
  const [selectedId, setSelectedId] = useState("");
  const [session, setSession] = useState(null);
  const [receipt, setReceipt] = useState(null);
  const [busy, setBusy] = useState("");
  const [error, setError] = useState("");
  const [restoreState, setRestoreState] = useState(repositoryRoot ? "restoring" : "idle");
  const generationRef = useRef(0);
  const pollRef = useRef(0);
  const pollBusyRef = useRef(false);
  const restoreStartedRef = useRef(false);
  const planRef = useRef(plan);

  useEffect(() => { planRef.current = plan; }, [plan]);

  const stopPolling = useCallback(() => {
    globalThis.clearInterval(pollRef.current);
    pollRef.current = 0;
    pollBusyRef.current = false;
  }, []);

  const inspect = useCallback(async (root, { restoring = false } = {}) => {
    if (!root) return false;
    const generation = ++generationRef.current;
    const hadPlan = Boolean(planRef.current);
    setBusy("inspect");
    setError("");
    if (restoring || !hadPlan) setRepositoryRoot(root);
    if (restoring) setRestoreState("restoring");

    try {
      const next = await tauriApi.inspectRepository(root);
      if (generation !== generationRef.current) return false;
      stopPolling();
      setRepositoryRoot(next.repository_root);
      setPlan(next);
      setSelectedId(selectDefaultTarget(next));
      setSession(null);
      setReceipt(null);
      setRestoreState("ready");
      globalThis.localStorage?.setItem(LAST_REPOSITORY_KEY, next.repository_root);
      return true;
    } catch (nextError) {
      if (generation !== generationRef.current) return false;
      setError(String(nextError));
      if (restoring || !hadPlan) {
        setPlan(null);
        setSelectedId("");
        setSession(null);
        setReceipt(null);
        setRestoreState("unavailable");
      }
      return false;
    } finally {
      if (generation === generationRef.current) setBusy("");
    }
  }, [stopPolling]);

  useEffect(() => {
    if (restoreStartedRef.current || !repositoryRoot) return;
    restoreStartedRef.current = true;
    if (!tauriApi.available()) {
      setRestoreState("unavailable");
      setError(t.desktopOnly);
      return;
    }
    void inspect(repositoryRoot, { restoring: true });
  }, [inspect, repositoryRoot, t.desktopOnly]);

  useEffect(() => () => stopPolling(), [stopPolling]);

  const chooseRepository = useCallback(async () => {
    if (!tauriApi.available()) {
      setError(t.desktopOnly);
      return false;
    }
    if (busy === "run") return false;
    try {
      const root = await tauriApi.pickRepository();
      return root ? inspect(root) : false;
    } catch (nextError) {
      setError(String(nextError));
      return false;
    }
  }, [busy, inspect, t.desktopOnly]);

  const retryRestore = useCallback(() => inspect(repositoryRoot, { restoring: true }), [inspect, repositoryRoot]);

  const startPolling = useCallback((sessionId, generation) => {
    stopPolling();
    pollRef.current = globalThis.setInterval(async () => {
      if (pollBusyRef.current || generation !== generationRef.current) return;
      pollBusyRef.current = true;
      try {
        const next = await tauriApi.readRunSession(sessionId);
        if (generation !== generationRef.current) return;
        setSession(next);
        if (TERMINAL_SESSION_STATES.has(next.status)) stopPolling();
      } catch (nextError) {
        if (generation === generationRef.current) setError(String(nextError));
      } finally {
        pollBusyRef.current = false;
      }
    }, 500);
  }, [stopPolling]);

  const target = useMemo(
    () => plan?.targets?.find((item) => item.id === selectedId) || null,
    [plan, selectedId],
  );

  const selectTarget = useCallback((id) => {
    if (busy === "run") return;
    setSelectedId(id);
    setSession(null);
    setReceipt(null);
    setError("");
  }, [busy]);

  const run = useCallback(async () => {
    if (!target || !repositoryRoot || busy) return;
    const generation = generationRef.current;
    let sessionId = "";
    setBusy("run");
    setError("");
    setReceipt(null);
    try {
      const created = await tauriApi.createRunSession(repositoryRoot, target.id);
      sessionId = created.id;
      if (generation !== generationRef.current) return;
      setSession(created);
      startPolling(created.id, generation);
      const nextReceipt = await tauriApi.executeRunSession(created.id);
      if (generation !== generationRef.current) return;
      setReceipt(nextReceipt);
      setSession(await tauriApi.readRunSession(created.id));
      onReceiptChanged?.();
    } catch (nextError) {
      if (generation === generationRef.current) {
        setError(String(nextError));
        if (sessionId) {
          const authoritative = await tauriApi.readRunSession(sessionId).catch(() => null);
          if (authoritative && generation === generationRef.current) setSession(authoritative);
        }
      }
    } finally {
      if (generation === generationRef.current) {
        stopPolling();
        setBusy("");
      }
    }
  }, [busy, onReceiptChanged, repositoryRoot, startPolling, stopPolling, target]);

  const cancel = useCallback(async () => {
    if (!session) return;
    setSession((current) => current ? { ...current, message: t.cancel } : current);
    try {
      const requested = await tauriApi.cancelRunSession(session.id);
      setSession(requested);
      const authoritative = await tauriApi.readRunSession(session.id).catch(() => null);
      if (authoritative) setSession(authoritative);
    } catch (nextError) {
      setError(String(nextError));
    }
  }, [session, t.cancel]);

  const exportReceipt = useCallback(async () => {
    if (!receipt) return;
    try { await tauriApi.exportReceipt(receipt.id); }
    catch (nextError) { setError(String(nextError)); }
  }, [receipt]);

  const exportTaskPack = useCallback(async () => {
    const blocker = target?.blockers?.[0] || receipt?.first_observed_blocker;
    if (!blocker || !target) return;
    try { await tauriApi.exportAgentTaskPack(repositoryRoot, target.id, blocker); }
    catch (nextError) { setError(String(nextError)); }
  }, [receipt, repositoryRoot, target]);

  const value = useMemo(() => ({
    repositoryRoot, plan, selectedId, target, session, receipt, busy, error, restoreState,
    chooseRepository, retryRestore, selectTarget, run, cancel, exportReceipt, exportTaskPack,
  }), [repositoryRoot, plan, selectedId, target, session, receipt, busy, error, restoreState,
    chooseRepository, retryRestore, selectTarget, run, cancel, exportReceipt, exportTaskPack]);

  return <VerificationWorkspaceContext.Provider value={value}>{children}</VerificationWorkspaceContext.Provider>;
}

export function useVerificationWorkspace() {
  const value = useContext(VerificationWorkspaceContext);
  if (!value) throw new Error("useVerificationWorkspace must be used inside VerificationWorkspaceProvider");
  return value;
}
