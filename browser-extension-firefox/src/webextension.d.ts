declare namespace BlockuntuWebExtension {
  type StorageItems = Record<string, unknown>;

  interface RuntimePort {
    postMessage(message: unknown): void;
    disconnect(): void;
    onMessage: ExtensionEvent<(message: unknown) => void>;
    onDisconnect: ExtensionEvent<(port: RuntimePort) => void>;
  }

  interface ExtensionEvent<TListener extends (...args: never[]) => void> {
    addListener(listener: TListener): void;
    removeListener(listener: TListener): void;
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
  }

  interface TabsApi {
    query(queryInfo: { url?: string | string[] }): Promise<Tab[]>;
    update(tabId: number, updateProperties: { url?: string }): Promise<unknown>;
    onRemoved: ExtensionEvent<(tabId: number) => void>;
  }

  interface StorageArea {
    get(keys?: string | string[] | StorageItems | null): Promise<StorageItems>;
    set(items: StorageItems): Promise<void>;
  }

  interface StorageApi {
    local: StorageArea;
  }

  interface BrowserApi {
    runtime: RuntimeApi;
    storage: StorageApi;
    webNavigation: WebNavigationApi;
    tabs: TabsApi;
  }
}

declare const browser: BlockuntuWebExtension.BrowserApi;
