// Verify the small Rust FFI layout/constants against the installed Windows SDK.
#include <Windows.h>
#include <WinHvPlatform.h>
#include <cstddef>
static_assert(sizeof(WHV_REGISTER_VALUE) == 16);
static_assert(alignof(WHV_REGISTER_VALUE) == 16);
static_assert(sizeof(WHV_RUN_VP_EXIT_CONTEXT) == 224);
static_assert(alignof(WHV_RUN_VP_EXIT_CONTEXT) == 16);
static_assert(offsetof(WHV_RUN_VP_EXIT_CONTEXT, ExitReason) == 0);
static_assert(offsetof(WHV_X64_SEGMENT_REGISTER, Limit) == 8);
static_assert(offsetof(WHV_X64_SEGMENT_REGISTER, Attributes) == 14);
static_assert(WHvPartitionPropertyCodeProcessorCount == 0x1fff);
static_assert(WHvX64RegisterRax == 0 && WHvX64RegisterRip == 0x10);
static_assert(WHvX64RegisterRflags == 0x11 && WHvX64RegisterCs == 0x13);
static_assert(WHvX64RegisterEs == 0x12 && WHvX64RegisterSs == 0x14 && WHvX64RegisterDs == 0x15);
static_assert(WHvX64RegisterCr0 == 0x1c && WHvX64RegisterCr3 == 0x1e && WHvX64RegisterCr4 == 0x1f);
static_assert(WHvRunVpExitReasonX64Halt == 8);
static_assert((WHvMapGpaRangeFlagRead | WHvMapGpaRangeFlagWrite | WHvMapGpaRangeFlagExecute) == 7);

static_assert(WHvRunVpExitReasonCanceled == 0x2001);
static_assert(WHvRunVpExitReasonX64IoPortAccess == 2);
static_assert(offsetof(WHV_RUN_VP_EXIT_CONTEXT, VpContext) == 8);
static_assert(offsetof(WHV_VP_EXIT_CONTEXT, Rip) == 24);
static_assert(offsetof(WHV_RUN_VP_EXIT_CONTEXT, IoPortAccess) == 48);
static_assert(offsetof(WHV_X64_IO_PORT_ACCESS_CONTEXT, AccessInfo) == 20);
static_assert(offsetof(WHV_X64_IO_PORT_ACCESS_CONTEXT, PortNumber) == 24);
static_assert(offsetof(WHV_X64_IO_PORT_ACCESS_CONTEXT, Rax) == 32);

static_assert(WHvX64RegisterEfer == 0x2001);
static_assert(WHvX64RegisterGdtr == 0x1b && WHvX64RegisterIdtr == 0x1a);
static_assert(WHvX64RegisterRsp == 4 && WHvX64RegisterFs == 0x16 && WHvX64RegisterGs == 0x17);
static_assert(WHvRunVpExitReasonUnrecoverableException == 4);
static_assert(sizeof(WHV_X64_TABLE_REGISTER) == 16);
static_assert(offsetof(WHV_X64_TABLE_REGISTER, Limit) == 6);
static_assert(offsetof(WHV_X64_TABLE_REGISTER, Base) == 8);
static_assert(offsetof(WHV_X64_SEGMENT_REGISTER, Selector) == 12);

static_assert(WHvRegisterPendingInterruption == 0x80000000);
static_assert(WHvX64PendingInterrupt == 0);
static_assert(sizeof(WHV_X64_PENDING_INTERRUPTION_REGISTER) == 8);

#include <WinHvEmulation.h>
static_assert(WHvRunVpExitReasonMemoryAccess == 1);
static_assert(sizeof(WHV_MEMORY_ACCESS_CONTEXT) == 40);
static_assert(offsetof(WHV_RUN_VP_EXIT_CONTEXT, MemoryAccess) == 48);
static_assert(offsetof(WHV_MEMORY_ACCESS_CONTEXT, Gpa) == 24);
static_assert(offsetof(WHV_MEMORY_ACCESS_CONTEXT, Gva) == 32);
static_assert(sizeof(WHV_EMULATOR_MEMORY_ACCESS_INFO) == 24);
static_assert(offsetof(WHV_EMULATOR_MEMORY_ACCESS_INFO, Data) == 10);
static_assert(sizeof(WHV_EMULATOR_CALLBACKS) == 48);
static_assert(offsetof(WHV_EMULATOR_CALLBACKS, WHvEmulatorTranslateGvaPage) == 40);
static_assert(sizeof(WHV_TRANSLATE_GVA_RESULT) == 8);

// Native software player FFI; mirrored by Rust's windows-only layout tests.
static_assert(sizeof(WNDCLASSW) == 72);
static_assert(offsetof(WNDCLASSW, lpfnWndProc) == 8);
static_assert(offsetof(WNDCLASSW, hInstance) == 24);
static_assert(offsetof(WNDCLASSW, lpszClassName) == 64);
static_assert(sizeof(MSG) == 48);
static_assert(offsetof(MSG, wParam) == 16);
static_assert(offsetof(MSG, lParam) == 24);
static_assert(offsetof(MSG, pt) == 36);
static_assert(sizeof(BITMAPINFOHEADER) == 40);
static_assert(sizeof(RECT) == 16);

#include <d3d11.h>
static_assert(sizeof(D3D11_TEXTURE2D_DESC) == 44);
static_assert(sizeof(D3D11_MAPPED_SUBRESOURCE) == 16);
static_assert(sizeof(D3D11_VIEWPORT) == 24);
static_assert(D3D_DRIVER_TYPE_HARDWARE == 1 && D3D_DRIVER_TYPE_WARP == 5);
static_assert(D3D11_SDK_VERSION == 7 && DXGI_FORMAT_R8G8B8A8_UNORM == 28);
static_assert(D3D11_BIND_RENDER_TARGET == 0x20 && D3D11_CPU_ACCESS_READ == 0x20000);

static_assert(sizeof(DXGI_ADAPTER_DESC1) == 312);
static_assert(offsetof(DXGI_ADAPTER_DESC1, Flags) == 304);

static_assert(WHvCapabilityCodeProcessorClockFrequency == 0x1004);
static_assert(WHvPartitionPropertyCodeCpuidResultList == 0x1004);
static_assert(sizeof(WHV_X64_CPUID_RESULT) == 32);
static_assert(offsetof(WHV_X64_CPUID_RESULT, Eax) == 16);
static_assert(offsetof(WHV_X64_CPUID_RESULT, Ebx) == 20);
static_assert(offsetof(WHV_X64_CPUID_RESULT, Ecx) == 24);
static_assert(offsetof(WHV_X64_CPUID_RESULT, Edx) == 28);

static_assert(WHvPartitionPropertyCodeLocalApicEmulationMode == 0x1005);
static_assert(WHvX64LocalApicEmulationModeXApic == 1);
static_assert(WHvX64RegisterApicBase == 0x2003);
static_assert(WHvRegisterInternalActivityState == 0x80000005);
static_assert(sizeof(WHV_INTERNAL_ACTIVITY_REGISTER) == 8);

static_assert(WHvPartitionPropertyCodeExtendedVmExits == 1);
static_assert(WHvPartitionPropertyCodeCpuidExitList == 0x1003);
static_assert(WHvRunVpExitReasonX64Cpuid == 0x1001);
static_assert(offsetof(WHV_RUN_VP_EXIT_CONTEXT, CpuidAccess) == 48);
static_assert(sizeof(WHV_X64_CPUID_ACCESS_CONTEXT) == 64);
static_assert(sizeof(WHV_INTERRUPT_CONTROL) == 16);
static_assert(offsetof(WHV_INTERRUPT_CONTROL, Destination) == 8);
static_assert(offsetof(WHV_INTERRUPT_CONTROL, Vector) == 12);
