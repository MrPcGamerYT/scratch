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
