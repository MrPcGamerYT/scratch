#include "rustcore.h"
#include <QByteArray>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonValue>
#include <QStringList>
#include <iostream>
int main() {
    NexusVmRequest request{"C:/Nexus/a \"quoted\" disk.qcow2", "", "C:/firmware/code.fd", "C:/user/a,b.fd", 4096, 4, 5555, 21};
    const auto size = nexus_vm_plan(&request, nullptr, 0);
    if (!size || size > 100000) return 1;
    QByteArray output(static_cast<int>(size), '\0');
    if (nexus_vm_plan(&request, reinterpret_cast<std::uint8_t *>(output.data()), size) != size) return 2;
    const auto doc = QJsonDocument::fromJson(output);
    if (!doc.isArray()) return 3;
    QStringList args;
    for (const auto value : doc.array()) { if (!value.isString()) return 4; args << value.toString(); }
    if (!args.contains(request.disk) || !args.contains("whpx,kernel-irqchip=off") || !args.contains("user,id=net0,hostfwd=tcp:127.0.0.1:5555-:5555")) return 5;
    if (!args.contains("if=pflash,format=raw,unit=1,file=C:/user/a,,b.fd")) return 6;
    if (!args.contains("dsound,id=nexus_audio") || !args.contains("hda-output,bus=nexus_hda.0,audiodev=nexus_audio")) return 9;
    if (nexus_trim_working_set(0) != 0) return 10;
    request.flags = 8;
    const auto softwareSize = nexus_vm_plan(&request, nullptr, 0);
    QByteArray software(static_cast<int>(softwareSize), '\0');
    nexus_vm_plan(&request, reinterpret_cast<std::uint8_t *>(software.data()), softwareSize);
    if (!software.contains("tcg,thread=multi") || software.contains("whpx")) return 8;
    request.flags = 63;
    const auto virtioSize = nexus_vm_plan(&request, nullptr, 0);
    QByteArray virtio(static_cast<int>(virtioSize), '\0');
    nexus_vm_plan(&request, reinterpret_cast<std::uint8_t *>(virtio.data()), virtioSize);
    if (!virtio.contains("virtio-vga,id=nexus_display") || !virtio.contains("display=nexus_display")) return 11;
    request.cpus = 0;
    if (nexus_vm_plan(&request, nullptr, 0)) return 7;
    std::cout << "Rust VM launch policy C ABI / Qt JSON roundtrip passed; no Android boot or speed claim.\n";
    return 0;
}
