// tools/generate_api_metadata.py generates this file from hashcodecs/_hashcodecs.pyi.

define_python_hasher!(
    PyMurmur3X86Hasher32,
    Murmur3X86Hasher32,
    "murmur3_x86_32",
    "x86 32-bit",
    "    >>> hasher = murmur3_x86_32(b'hello', seed=7)\n    >>> hasher.update(b' world')\n    >>> hasher.hexdigest() == hasher.digest().hex()\n    True",
    4,
    4,
    |state: &Murmur3X86Hasher32| state.digest().to_le_bytes(),
);

define_python_hasher!(
    PyMurmur3X86Hasher128,
    Murmur3X86Hasher128,
    "murmur3_x86_128",
    "x86 128-bit",
    "    >>> hasher = murmur3_x86_128(b'hello', seed=7)\n    >>> hasher.update(b' world')\n    >>> len(hasher.digest())\n    16",
    16,
    16,
    |state: &Murmur3X86Hasher128| x86_128_digest(state.digest()),
);

define_python_hasher!(
    PyMurmur3X64Hasher128,
    Murmur3X64Hasher128,
    "murmur3_x64_128",
    "x64 128-bit",
    "    >>> hasher = murmur3_x64_128(b'hello', seed=7)\n    >>> checkpoint = hasher.copy()\n    >>> hasher.update(b' world')\n    >>> hasher.digest() != checkpoint.digest()\n    True",
    16,
    16,
    |state: &Murmur3X64Hasher128| x64_128_digest(state.digest()),
);
