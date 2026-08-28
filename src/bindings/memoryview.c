#include <Python.h>

PyObject *hashcodecs_memoryview_owner(PyObject *memoryview) {
    return PyMemoryView_GET_BASE(memoryview);
}
