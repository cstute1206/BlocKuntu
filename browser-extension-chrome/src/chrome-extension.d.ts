declare namespace BlockuntuChromeExtension {
  type StorageItems = Record<string, unknown>;

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
    onHistoryStateUpdated: ExtensionEvent<(details: NavigationDetails) => void>;
  }

  interface Tab {
    id?: number;
    url?: string;
    active?: boolean;
  }

  interface TabsApi {
    query(
      queryInfo: { url?: string | string[]; active?: boolean },
      callback: (tabs: Tab[]) => void
    ): void;
    update(
      tabId: number,
      updateProperties: { url?: string },
      callback?: () => void
    ): void;
    onRemoved: ExtensionEvent<(tabId: number) => void>;
    onActivated: ExtensionEvent<(activeInfo: { tabId: number }) => void>;
  }

  interface Alarm {
    name: string;
  }

  interface AlarmsApi {
    create(name: string, alarmInfo: { periodInMinutes: number }): void;
    onAlarm: ExtensionEvent<(alarm: Alarm) => void>;
  }

  interface StorageArea {
    get(keys: string | string[] | StorageItems | null, callback: (items: StorageItems) => void): void;
    set(items: StorageItems, callback?: () => void): void;
  }

  interface StorageApi {
    local: StorageArea;
  }

  interface ChromeApi {
    alarms: AlarmsApi;
    runtime: RuntimeApi;
    storage: StorageApi;
    tabs: TabsApi;
    webNavigation: WebNavigationApi;
  }
}

declare const chrome: BlockuntuChromeExtension.ChromeApi;
