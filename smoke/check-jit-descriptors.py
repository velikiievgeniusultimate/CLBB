"""Register a QEMU code region with the ART debugger interface already visible."""
import ctypes
import sys


class Descriptor(ctypes.Structure):
    _fields_ = [
        ("version", ctypes.c_uint32),
        ("action", ctypes.c_uint32),
        ("relevant", ctypes.c_void_p),
        ("first", ctypes.c_void_p),
    ]


art = ctypes.CDLL(sys.argv[1], mode=ctypes.RTLD_GLOBAL)
bridge = ctypes.CDLL(sys.argv[2])
art_descriptor = Descriptor.in_dll(art, "__jit_debug_descriptor")
qemu_descriptor = Descriptor.in_dll(bridge, "__jit_debug_descriptor")
assert ctypes.addressof(art_descriptor) != ctypes.addressof(qemu_descriptor)
original_art = bytes(art_descriptor)
code = ctypes.create_string_buffer(64)
bridge.tcg_register_jit.argtypes = [ctypes.c_void_p, ctypes.c_size_t]
bridge.tcg_register_jit.restype = None
bridge.tcg_register_jit(code, ctypes.sizeof(code))
assert bytes(art_descriptor) == original_art
assert qemu_descriptor.version == 1 and qemu_descriptor.action == 1
assert qemu_descriptor.first and qemu_descriptor.first == qemu_descriptor.relevant
print("Independent ART and QEMU debugger descriptors verified.")
