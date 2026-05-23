import { listen } from "@tauri-apps/api/event";
import { useEffect, useMemo, useRef, useState } from "react";
import type { Settings } from "../domain/session";
import { getRestOverlayRequest, getSettings, hideRestOverlay, readRestOverlayHtml, readRestOverlayImageDataUrl } from "../storage/tauriApi";
import type { RestOverlayRequest } from "../storage/tauriApi";

const restOverlayShowEvent = "rest-overlay:show";

const defaultSettings: Settings = {
  dailyGoalMinutes: 240,
  dailyResetMinutes: 0,
  includeWeekendsInStreak: true,
  focusMinutes: 30,
  restMinutes: 5,
  skipRest: false,
  restOverlayMode: "blur",
  restOverlayImage: null,
  restOverlayHtml: null,
  gitSyncRepoPath: null,
};

export function RestOverlay() {
  const [settings, setSettings] = useState<Settings>(defaultSettings);
  const [request, setRequest] = useState<RestOverlayRequest | null>(null);
  const [htmlDocument, setHtmlDocument] = useState<string | null>(null);
  const [imageSource, setImageSource] = useState<string | null>(null);
  const closeTimer = useRef<number | null>(null);

  useEffect(() => {
    let alive = true;
    let unlisten: (() => void) | null = null;

    async function activate(next: RestOverlayRequest) {
      if (!next.visible) {
        return;
      }

      const latestSettings = await getSettings().catch(() => null);
      if (!alive) {
        return;
      }

      if (latestSettings) {
        setSettings(latestSettings);
      }
      setRequest(next);
    }

    void listen<RestOverlayRequest>(restOverlayShowEvent, (event) => {
      void activate(event.payload);
    })
      .then((nextUnlisten) => {
        if (alive) {
          unlisten = nextUnlisten;
        } else {
          nextUnlisten();
        }
      })
      .catch(() => undefined);

    void getRestOverlayRequest()
      .then((next) => activate(next))
      .catch(() => undefined);

    return () => {
      alive = false;
      unlisten?.();
    };
  }, []);

  useEffect(() => {
    if (closeTimer.current) {
      window.clearTimeout(closeTimer.current);
      closeTimer.current = null;
    }

    if (!request?.visible) {
      return;
    }

    closeTimer.current = window.setTimeout(() => {
      closeOverlay();
    }, Math.max(1, request.durationSeconds) * 1000);

    return () => {
      if (closeTimer.current) {
        window.clearTimeout(closeTimer.current);
        closeTimer.current = null;
      }
    };
  }, [request?.requestId, request?.visible, request?.durationSeconds]);

  useEffect(() => {
    let alive = true;
    setHtmlDocument(null);

    if (settings.restOverlayMode !== "html") {
      return () => {
        alive = false;
      };
    }

    const html = normalizeOptional(settings.restOverlayHtml);
    if (!html) {
      return () => {
        alive = false;
      };
    }

    if (html.startsWith("<")) {
      setHtmlDocument(html);
      return () => {
        alive = false;
      };
    }

    void readRestOverlayHtml()
      .then((nextHtml) => {
        if (alive) {
          setHtmlDocument(nextHtml);
        }
      })
      .catch(() => {
        if (alive) {
          setHtmlDocument(null);
        }
      });

    return () => {
      alive = false;
    };
  }, [settings.restOverlayMode, settings.restOverlayHtml]);

  useEffect(() => {
    let alive = true;
    setImageSource(null);

    if (settings.restOverlayMode !== "image") {
      return () => {
        alive = false;
      };
    }

    const image = normalizeOptional(settings.restOverlayImage);
    if (!image) {
      return () => {
        alive = false;
      };
    }

    if (/^(https?:|data:|blob:)/i.test(image)) {
      setImageSource(image);
      return () => {
        alive = false;
      };
    }

    void readRestOverlayImageDataUrl()
      .then((nextImageSource) => {
        if (alive) {
          setImageSource(nextImageSource);
        }
      })
      .catch(() => {
        if (alive) {
          setImageSource(null);
        }
      });

    return () => {
      alive = false;
    };
  }, [settings.restOverlayMode, settings.restOverlayImage]);

  const content = useMemo(() => overlayContent(settings, htmlDocument, imageSource), [settings, htmlDocument, imageSource]);

  function closeOverlay() {
    setRequest(null);
    void hideRestOverlay().catch(() => undefined);
  }

  return (
    <main className={`rest-overlay-screen ${content.className}`}>
      {content.mode === "image" && <img className="rest-overlay-image" src={content.source} alt="" />}
      {content.mode === "html" && (
        <iframe className="rest-overlay-html" title="Rest overlay content" srcDoc={content.html} sandbox="allow-scripts" />
      )}
      <button className="rest-overlay-close" onClick={closeOverlay} aria-label="Close rest overlay" title="Close">
        <span />
        <span />
      </button>
    </main>
  );
}

type OverlayContent =
  | { mode: "blur"; className: string }
  | { mode: "image"; className: string; source: string }
  | { mode: "html"; className: string; html: string };

function overlayContent(settings: Settings, htmlDocument: string | null, imageSource: string | null): OverlayContent {
  if (settings.restOverlayMode === "image") {
    if (imageSource) {
      return { mode: "image", className: "rest-overlay-image-mode", source: imageSource };
    }
  }

  if (settings.restOverlayMode === "html") {
    if (htmlDocument) {
      return { mode: "html", className: "rest-overlay-html-mode", html: htmlDocument };
    }
  }

  return { mode: "blur", className: "rest-overlay-blur-mode" };
}

function normalizeOptional(value: string | null): string | null {
  const trimmed = value?.trim();
  return trimmed ? trimmed : null;
}
