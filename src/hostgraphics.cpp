#include "hostgraphics.h"
#ifndef NOMINMAX
#define NOMINMAX
#endif
#include <windows.h>
#include <dxgi1_2.h>
#include <QStringList>

QString hostGraphicsReport() {
    QStringList lines{"Host GPU inventory (DXGI; not an Android rendering test):"};
    IDXGIFactory1 *factory = nullptr;
    const HRESULT created = CreateDXGIFactory1(__uuidof(IDXGIFactory1), reinterpret_cast<void **>(&factory));
    if (FAILED(created) || !factory) return lines.join('\n') + "\nDXGI enumeration unavailable.";
    for (UINT i = 0; i < 32; ++i) {
        IDXGIAdapter1 *adapter = nullptr;
        const HRESULT result = factory->EnumAdapters1(i, &adapter);
        if (result == DXGI_ERROR_NOT_FOUND) break;
        if (FAILED(result) || !adapter) { lines << "Adapter enumeration failed."; break; }
        DXGI_ADAPTER_DESC1 desc{};
        if (SUCCEEDED(adapter->GetDesc1(&desc))) {
            lines << QString("%1: %2 · dedicated VRAM %3 MB · shared system memory capacity %4 MB · %5")
                .arg(i).arg(QString::fromWCharArray(desc.Description))
                .arg(static_cast<qulonglong>(desc.DedicatedVideoMemory / (1024 * 1024)))
                .arg(static_cast<qulonglong>(desc.SharedSystemMemory / (1024 * 1024)))
                .arg(desc.Flags & DXGI_ADAPTER_FLAG_SOFTWARE ? "software adapter" : "hardware adapter");
        }
        adapter->Release();
    }
    factory->Release();
    lines << "Window GPU preference: automatic / Windows-managed."
          << "Android GPU selection: unavailable — current std/virtio scanout has no accelerated host renderer."
          << "Adapter memory figures are capabilities, not usage or proof that a GPU is rendering this guest.";
    return lines.join('\n');
}
