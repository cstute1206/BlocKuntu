declare namespace BlockuntuChromeExtension {
  interface ExtensionEvent<TListener extends (...args: never[]) => void> {
    addListener(listener: TListener): void;
    removeListener(listener: TListener): void;
  }

  interface RuntimePort {
    postMessage(message: unknown): void;
    disconnect(): void;
    onMessage: ExtensionEvent<(message: unknown) => void>;
    onDisconnect: ExtensionEvent<(port: RuntimePort) => void>;
  }

  interface RuntimeManifest {
    version: string;
  }

  interface RuntimeApi {
    id: string;
    lastError?: { message?: string };
    connectNative(name: string): RuntimePort;
    getManifest(): RuntimeManifest;
    getURL(path: string): string;
  }

  interface NavigationDetails {
    tabId: number;
    frameId: number;
    url: string;
  }

  interface WebNavigationApi {
    onBeforeNavigate: ExtensionEvent<(details: NavigationDetails) => void>;
  }

  interface TabsApi {
    update(
      tabId: number,
      updateProperties: { url?: string },
      callback?: () => void
    ): void;
    onRemoved: ExtensionEvent<(tabId: number) => void>;
  }

  interface Alarm {
    name: string;
  }

  interface AlarmsApi {
    create(name: string, alarmInfo: { periodInMinutes: number }): void;
    onAlarm: ExtensionEvent<(alarm: Alarm) => void>;
  }

  interface ChromeApi {
    alarms: AlarmsApi;
    runtime: RuntimeApi;
    tabs: TabsApi;
    webNavigation: WebNavigationApi;
  }
}

declare const chrome: BlockuntuChromeExtension.ChromeApi;
