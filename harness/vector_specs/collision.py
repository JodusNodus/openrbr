"""E33 vector leaves used by the body and collision code."""

SET_NAME = "collision"

import struct  # noqa: E402


def _vec(data, offset=0):
    return struct.unpack_from("<4f", data, offset)


def _all_below_zero(args, windows):
    values = struct.unpack_from("<3f", windows[0][3])
    return "all_below_zero" if all(value < 0.0 for value in values) else "other"


def _box_hit(args, windows):
    a, b, ext, tol = (_vec(win[3]) for win in windows)
    half = tuple(f32_round(f32_round(b[k] - a[k]) * 0.5) for k in range(4))
    dx, dy, dz = half[:3]
    sx = f32_round(f32_round(a[0] + dx) - ext[0])
    sy = f32_round(f32_round(a[1] + dy) - ext[1])
    sz = f32_round(f32_round(a[2] + dz) - ext[2])
    if abs(sx) > abs(dx) + tol[0] or abs(sy) > abs(dy) + tol[1] or abs(sz) > abs(dz) + tol[2]:
        return "miss_axis"
    cr = (dy * sz - dz * sy, dz * sx - dx * sz, dx * sy - dy * sx)
    if (abs(cr[0]) > abs(dy) * tol[2] + abs(dz) * tol[1] or
            abs(cr[1]) > abs(dz) * tol[0] + abs(dx) * tol[2] or
            abs(cr[2]) > abs(dy) * tol[0] + abs(dx) * tol[1]):
        return "miss_cross"
    return "hit"


def _fuzz_all_below_zero(mu, args, rng, put):
    values = [rng.uniform(-100.0, -0.01) for _ in range(3)]
    if rng.randrange(2):
        values[rng.randrange(3)] = rng.choice((0.0, 0.01, 100.0, float("nan")))
    for offset, value in enumerate(values):
        put(args[0] + 4 * offset, value)


def _fuzz_box_hit(mu, args, rng, put):
    a, b, ext, tol = args
    if rng.randrange(2):
        # Coincident centres with a generous tolerance always pass both tests.
        for ptr in (a, b, ext):
            for offset in range(4):
                put(ptr + offset * 4, 0.0)
        for offset in range(3):
            put(tol + offset * 4, 1.0)
    else:
        # Separated points fail the first axis check.
        for ptr in (a, b):
            for offset in range(3):
                put(ptr + offset * 4, 100.0)
        for ptr in (ext, tol):
            for offset in range(3):
                put(ptr + offset * 4, 0.0)


def _sample_a_nodes(mu, args):
    grid, index, t_word = args[:3]
    word = lambda at: struct.unpack("<I", bytes(mu.mem_read(at, 4)))[0]
    half = lambda at: struct.unpack("<H", bytes(mu.mem_read(at, 2)))[0]
    if not word(grid + 0x400):
        return []
    pair = bytes(mu.mem_read(grid + 0x484 + index * 6, 6))
    data = word(grid + 0x400)
    pb0, pb1 = data + pair[4] * 2, data + pair[5] * 2
    x0, y0 = struct.unpack("<2B", bytes(mu.mem_read(pb0, 2)))
    x1, y1 = struct.unpack("<2B", bytes(mu.mem_read(pb1, 2)))
    t = f32(t_word)
    one_minus_t = f32_round(1.0 - t)
    fx = f32_round(f32_round(f32_round(x1 * one_minus_t) + f32_round(x0 * t)) * (1.0 / 256.0))
    fy = f32_round(f32_round(f32_round(y0 * t) + f32_round(y1 * one_minus_t)) * (1.0 / 256.0))

    def cvtt(value):
        if not math.isfinite(value) or value < -2147483648.0 or value >= 2147483648.0:
            return -2147483648
        return int(value)

    width, height = half(grid + 0x404), half(grid + 0x406)
    a_index = cvtt(f32_round(width * fy)) * width + cvtt(f32_round(width * fx))
    b_index = cvtt(f32_round(height * fy)) * height + cvtt(f32_round(height * fx))
    return [(data + pair[4] * 2, 2), (data + pair[5] * 2, 2),
            (word(grid + 0x408) + a_index & 0xFFFFFFFF, 1),
            (word(grid + 0x40C) + b_index & 0xFFFFFFFF, 1)]


def _sample_a_branch(args, windows):
    grid = windows[0][3]
    return "empty" if struct.unpack_from("<I", grid, 0x400)[0] == 0 else "sampled"


def _sample_a_direct_args(rng, this):
    t = rng.choice((0.0, 0.25, 0.5, 0.75, 1.0))
    t_bits = struct.unpack("<I", struct.pack("<f", t))[0]
    return [0, t_bits, this + 0x6a0, this + 0x6a4]


def _read_clear_direct_args(_rng, this):
    return [this + 0x6a0, this + 0x6a4]


def _store_vec_direct_args(_rng, this):
    return [this + 0x6a0]


def _edge_at_direct_args(rng, _this):
    return [rng.choice((-5, -4, -1, 0, 1, 55)) & 0xffffffff]


def _edge_pair_direct_args(_rng, _this):
    return [0]


def _link_nodes(mu, args):
    link = args[0]
    read = lambda at: struct.unpack("<I", bytes(mu.mem_read(at, 4)))[0]
    pointers = (link, read(link), read(link + 4), read(link + 8))
    return [(at, 0x10) for at in dict.fromkeys(p for p in pointers if p)]


def _fuzz_unlink(_mu, args, rng, put):
    link = args[0]
    nxt, prev, owner = link + 0x20, link + 0x40, link + 0x60
    case = rng.randrange(5)
    next_ptr, prev_ptr, owner_ptr = (nxt, prev, owner)
    if case == 0:  # unattached link
        owner_ptr = 0
    elif case == 1:  # sole member
        next_ptr = prev_ptr = 0
    elif case == 2:  # head with a successor
        prev_ptr = 0
    elif case == 3:  # tail with a predecessor
        next_ptr = 0
    put(link, next_ptr, "<I")
    put(link + 4, prev_ptr, "<I")
    put(link + 8, owner_ptr, "<I")
    put(link + 12, 0x12345678, "<I")
    put(owner, next_ptr if prev_ptr == 0 else link, "<I")
    put(owner + 4, prev_ptr if next_ptr == 0 else nxt, "<I")
    put(nxt, 0, "<I")
    put(nxt + 4, link, "<I")
    put(nxt + 8, owner, "<I")
    put(prev, link, "<I")
    put(prev + 4, 0, "<I")
    put(prev + 8, owner, "<I")


def _dict_entry_direct_args(rng, _this):
    return [rng.choice((-2, -1, 0, 1, 3, 4, 8)) & 0xFFFFFFFF]


def _fuzz_dict(_mu, args, rng, put):
    this = args[0]
    parent = this + 0x40
    has_parent = rng.randrange(4) != 0
    put(this, 0x737B98, "<I")
    put(this + 8, this + 0x180, "<I")
    put(this + 0x0C, this + 0x1C0, "<I")
    put(this + 0x10, parent if has_parent else 0, "<I")
    if has_parent:
        put(parent, 0x737B98, "<I")
        put(parent + 8, rng.choice((0, 1, 3, 7)), "<I")
        put(parent + 0x0C, this + 0x240, "<I")
        put(parent + 0x10, 0, "<I")


def _dict_count_dispatch(mu, args):
    parent = struct.unpack("<I", bytes(mu.mem_read(args[0] + 0x10, 4)))[0]
    return [(vtarget(mu, parent, 3), 0)] if parent else []


def _dict_entry_dispatch(mu, args):
    parent = struct.unpack("<I", bytes(mu.mem_read(args[0] + 0x10, 4)))[0]
    if not parent:
        return []
    word = lambda at: struct.unpack("<I", bytes(mu.mem_read(at, 4)))[0]
    count = word(parent + 8)
    if args[1] < count:
        return [(vtarget(mu, parent, 3), 0), (vtarget(mu, parent, 4), 4)]
    return [(vtarget(mu, parent, 3), 0), (vtarget(mu, parent, 3), 0)]


def _dict_lookup_direct_args(_rng, this):
    return [this + 0x80, this + 0x90]


def _make_event_nodes(mu, _args):
    stack = mu.reg_read(UC_X86_REG_ESP)
    return [(stack - 0x1C, 4)]


def _make_event_dispatch(mu, args):
    return [(vtarget(mu, args[4], 70), 0), (vtarget(mu, args[3], 70), 0)]


def _state_unka44_dispatch(mu, args):
    owner = word(mu, args[0] + 0x30)
    return [(vtarget(mu, owner, 12), 0)] if owner else []


def _body_clear_grid_direct_args(_rng, body):
    return [body, body + 0x80, body + 0x90]


def _select_table_direct_args(_rng, this):
    return [this]


def _body_vf44_direct_args(_rng, body):
    return [body]


def _body_vfcc_direct_args(_rng, body):
    return [body + 0x100, body + 0x110]


def _brake_wall_direct_fuzz(_mu, heap, rng):
    wall, lower, upper = heap + 0x10000, heap + 0x12000, heap + 0x12010
    if rng.randrange(5) == 0:
        upper = lower
    root = bytearray(0x21C)
    struct.pack_into("<I", root, 0, 0x737E04)
    bounds = [rng.uniform(-1000.0, 1000.0) for _ in range(4)]
    struct.pack_into("<4f", root, 0x64, *bounds)
    output = lambda: struct.pack("<4f", *(rng.uniform(-1000.0, 1000.0) for _ in range(4)))
    writes = [(wall, bytes(root)), (lower, output()), (upper, output())]
    return [lower, upper], writes, wall


def _brake_wall_query_direct_fuzz(_mu, heap, rng, box_query):
    wall, children, grandchildren = heap + 0x10000, heap + 0x11000, heap + 0x11200
    shape, collector = heap + 0x13000, heap + 0x14000
    root = bytearray(0x21C)
    struct.pack_into("<I", root, 0, 0x737E04)
    case = rng.randrange(4)
    child_ptr = children if case >= 2 else 0
    root_items = 1 if case == 0 else 0
    struct.pack_into("<II", root, 0x74, root_items, child_ptr)

    writes = [(wall, bytes(root)), (collector, bytes(rng.getrandbits(8) for _ in range(0x194)))]
    if child_ptr:
        def node(cx, cy, ex, ey, items=1, child=0):
            return struct.pack("<4fII", cx, cy, ex, ey, items, child)

        nested = case == 3
        writes.append((children, node(0.0, 0.0, 5.0, 4.0, 0 if nested else 1,
                                      grandchildren if nested else 0) +
                       node(10.0, 0.0, 4.0, 4.0)))
        if nested:
            writes.append((grandchildren, node(-2.0, 0.0, 2.0, 2.0) +
                           node(2.0, 0.0, 2.0, 2.0)))

    centers = ((0.0, 0.0), (4.999, 0.0), (5.0, 0.0), (7.0, 0.0),
               (10.0, 0.0), (14.999, 0.0), (15.0, 0.0), (50.0, 50.0),
               (-2.0, 0.0), (2.0, 0.0))
    x, y = rng.choice(centers)
    if box_query:
        ex, ey = rng.choice((0.0, 0.5, 1.0, 4.0)), rng.choice((0.0, 0.5, 1.0))
    else:
        ex = ey = 0.0
    writes.append((shape, struct.pack("<4f", x, y, ex, ey)))
    return [shape, collector], writes, wall


def _brake_wall_query_point_fuzz(mu, heap, rng):
    return _brake_wall_query_direct_fuzz(mu, heap, rng, False)


def _brake_wall_query_box_fuzz(mu, heap, rng):
    return _brake_wall_query_direct_fuzz(mu, heap, rng, True)


def _brake_wall_vf15c_direct_fuzz(mu, heap, rng):
    args, writes, wall = _brake_wall_query_direct_fuzz(mu, heap, rng, True)
    point, collector = args
    original = next(data for at, data in writes if at == point)
    x, y = struct.unpack_from("<2f", original)
    writes = [(at, data) for at, data in writes if at != collector]
    writes = [(at, struct.pack("<4f", x, y, rng.uniform(-5.0, 5.0), rng.uniform(-5.0, 5.0))
               if at == point else data) for at, data in writes]
    return [point], writes, wall


def _brake_wall_add_count_direct_fuzz(_mu, heap, rng):
    wall, data = heap + 0x10000, heap + 0x11000
    block = bytearray(0x400)

    def node(offset, items, child):
        struct.pack_into("<4fII", block, offset, rng.uniform(-100.0, 100.0),
                         rng.uniform(-100.0, 100.0), 1.0, 1.0, items, child)

    case = rng.randrange(5)
    if case == 0:
        node(0, 1, 0x350)
    elif case == 1:
        node(0, 0, 0)
    else:
        node(0, 0, 0x100)
        if case == 2:
            node(0x100, 1, 0x300)
            node(0x118, 1, 0x320)
        else:
            node(0x100, 0, 0x200 if case == 4 else 0x280)
            node(0x118, 1, 0x340)
            if case == 3:
                node(0x280, 1, 0x360)
                node(0x298, 1, 0x380)
            else:
                node(0x200, 0, 0x300)
                node(0x218, 1, 0x340)
                node(0x300, 1, 0x360)
                node(0x318, 1, 0x380)
    return [data, data], [(data, bytes(block))], wall


def _e36_sphere_box_direct_fuzz(_mu, heap, rng):
    body, vector, index = heap + 0x10000, heap + 0x20000, heap + 0x30000
    obj = bytearray(rng.getrandbits(8) for _ in range(0x300))
    center = [float(rng.randrange(-3, 4)) for _ in range(3)]
    center_w = float(rng.choice((-2, -1, 1, 2)))
    extents = [rng.choice((0.0, 0.25, 0.5, 1.0, 2.0, 4.0)) for _ in range(3)]
    radius = rng.choice((0.0, 0.125, 0.25, 0.5, 1.0, 2.0))
    axes = rng.choice(((0, 1, 2), (1, 0, 2), (2, 1, 0)))
    signs = rng.choice(((1.0, 1.0, 1.0), (-1.0, 1.0, 1.0),
                        (1.0, -1.0, 1.0), (1.0, 1.0, -1.0)))
    for offset, value in zip((0x10, 0x14, 0x18, 0x1C), (*center, center_w)):
        struct.pack_into("<f", obj, offset, value)
    for offset, value in zip((0x20, 0x24, 0x28), extents):
        struct.pack_into("<f", obj, offset, value)
    for row in range(3):
        basis = [0.0, 0.0, 0.0, float(rng.randrange(-2, 3))]
        basis[axes[row]] = signs[row]
        struct.pack_into("<4f", obj, 0x30 + row * 0x10, *basis)

    mode = rng.randrange(8)
    local = [float(rng.randrange(-2, 3)) / 2 for _ in range(3)]
    if mode == 0:
        local = [0.0, 0.0, 0.0]
    elif mode == 1:
        local[0] = extents[0] + radius * 0.5
    elif mode == 2:
        local[0] = extents[0] + radius
    elif mode == 3:
        local[0] = extents[0] + radius + 0.25
    elif mode == 4:
        local[1] = -(extents[1] + radius * 0.5)
    elif mode == 5:
        local[2] = extents[2] + radius * 0.5
    if local == [0.0, 0.0, 0.0]:
        local[0] = 0.25
    world = list(center)
    for row in range(3):
        world[axes[row]] += signs[row] * local[row]
    vec = struct.pack("<5f", *world, float(rng.randrange(-2, 3)), radius)
    idx = struct.pack("<I", rng.randrange(3))
    return [vector, index], [(body, bytes(obj)), (vector, vec), (index, idx)], body


def _e36_obb_test_direct_fuzz(_mu, heap, rng):
    body, vector, index = heap + 0x10000, heap + 0x20000, heap + 0x30000
    index_value = rng.randrange(3)
    obj = bytearray(0x300)
    center = [float(rng.randrange(-3, 4)) for _ in range(3)]
    degenerate = rng.randrange(20) == 0
    extents = [0.0, 0.0, 0.0] if degenerate else [rng.choice((0.5, 1.0, 2.0, 4.0)) for _ in range(3)]
    center_w = rng.choice((-2.0, -0.5, 0.25, 1.0, 3.0))
    for offset, value in zip((0x10, 0x14, 0x18, 0x1C), (*center, center_w)):
        struct.pack_into("<f", obj, offset, value)
    for offset, value in zip((0x20, 0x24, 0x28), extents):
        struct.pack_into("<f", obj, offset, value)
    struct.pack_into("<4f", obj, 0x30, 1.0, 0.0, 0.0, 0.25)
    struct.pack_into("<4f", obj, 0x40, 0.0, 1.0, 0.0, -0.5)
    struct.pack_into("<4f", obj, 0x50, 0.0, 0.0, 1.0, 2.0)
    slot = index_value * 0x40
    struct.pack_into("<f", obj, slot + 0x9C, rng.choice((-2.0, -0.25, 0.5, 1.5)))
    struct.pack_into("<f", obj, slot + 0xAC, rng.choice((-1.5, -0.125, 0.25, 2.0)))

    query_extents = [0.0, 0.0, 0.0] if degenerate else [rng.choice((0.25, 0.5, 1.0, 2.0)) for _ in range(3)]
    orientation = 0 if degenerate else rng.randrange(3)
    if orientation == 0:
        axes = ((1.0, 0.0, 0.0), (0.0, 1.0, 0.0), (0.0, 0.0, 1.0))
    elif orientation == 1:
        c = 0.7071067690849304
        axes = ((c, c, 0.0), (-c, c, 0.0), (0.0, 0.0, 1.0))
    else:
        # A non-axis-aligned orthonormal basis exercises all three query axes.
        c = 0.8660253882408142
        s = 0.5
        axes = ((c, 0.0, s), (0.0, 1.0, 0.0), (-s, 0.0, c))

    mode = rng.randrange(8)
    offset = [0.0, 0.0, 0.0]
    if mode == 0:
        offset = [0.125, -0.25, 0.375]
    elif mode == 1:
        offset = [0.25, 0.125, -0.25]
    elif mode == 2:
        offset[0] = extents[0] + query_extents[0] + 0.5
    elif mode == 3:
        offset[0] = extents[0] + query_extents[0]
    elif mode == 4:
        offset[1] = -(extents[1] + query_extents[1] + 0.5)
    elif mode == 5:
        offset = [rng.choice((-1.0, 1.0)) * rng.random() for _ in range(3)]
    elif mode == 6:
        offset = [extents[0] * 0.5, extents[1] * 0.25, extents[2] * 0.125]
    else:
        offset = [extents[0] + query_extents[0] + 1.0,
                  extents[1] + query_extents[1] + 0.75,
                  extents[2] + query_extents[2] + 0.5]
    if degenerate:
        offset = [0.0, 0.0, 0.0]
    world = [center[i] + offset[i] for i in range(3)]
    vec = bytearray(0x50)
    struct.pack_into("<4f", vec, 0, *world, rng.choice((-3.0, -0.25, 0.5, 2.0)))
    struct.pack_into("<4f", vec, 0x10, *query_extents, 1.0)
    for row, axis in enumerate(axes):
        struct.pack_into("<4f", vec, 0x20 + row * 0x10, *axis, float(row + 1))
    idx = struct.pack("<I", index_value)
    return [vector, index], [(body, bytes(obj)), (vector, bytes(vec)), (index, idx)], body


def _e36_get680_direct_fuzz(_mu, heap, rng):
    body = heap + 0x10000
    obj = bytearray(rng.getrandbits(8) for _ in range(0x690))
    struct.pack_into("<I", obj, 0x680, rng.choice((0, 1, 0x12345678, 0xFFFFFFFF)))
    return [], [(body, bytes(obj))], body


def _e36_get68a_direct_fuzz(_mu, heap, rng):
    body = heap + 0x10000
    obj = bytearray(rng.getrandbits(8) for _ in range(0x690))
    obj[0x68A] = rng.choice((0, 1, 4, 8, 0xFC, 0xFF))
    return [], [(body, bytes(obj))], body


def _e36_collide_init_direct_fuzz(_mu, heap, rng):
    body = heap + 0x10000
    obj = bytes(rng.getrandbits(8) for _ in range(0x90))
    return [], [(body, obj)], body


def _e36_nop_direct_fuzz(_mu, _heap, _rng):
    return [], [], 0


def _e36_alloc_event_direct_fuzz(_mu, heap, rng):
    event, next_event = heap + 0x10000, heap + 0x11000
    case = rng.randrange(3)
    size = rng.choice((0x20, 0x80, 0xF0, 0x180))
    count = rng.choice((0, 1, 2, 0x7FFFFFFF, 0xFFFFFFFF))
    writes = [(0x008EF7F4, struct.pack("<3I", 0, 0, count))]
    if case == 0:  # An empty pool delegates to operator new.
        return [size], writes, 0

    owner = 0x008EF7F4 if case == 2 else heap + 0x14000
    has_next = case == 2 and rng.randrange(2) != 0
    event_image = bytearray(rng.getrandbits(8) for _ in range(0x20))
    struct.pack_into("<3I", event_image, 0x0C, next_event if has_next else 0, 0, owner)
    if case == 2:
        writes[0] = (0x008EF7F4,
                     struct.pack("<3I", event, next_event if has_next else event, count))
        writes.append((next_event, bytes(rng.getrandbits(8) for _ in range(0x20))))
        if has_next:
            successor = bytearray(rng.getrandbits(8) for _ in range(0x20))
            struct.pack_into("<I", successor, 0x10, event)
            writes[-1] = (next_event, bytes(successor))
    else:  # A stale pool head whose link belongs to another list is returned unchanged.
        writes[0] = (0x008EF7F4, struct.pack("<3I", event, event, count))
    writes.append((event, bytes(event_image)))
    return [size], writes, 0


def _e36_alloc_event_nodes(mu, args):
    event = _read_u32(mu, 0x008EF7F4)
    nodes = [(0x008EF7F4, 12)]
    if event:
        nodes.append((event, 0x20))
        next_event = _read_u32(mu, event + 0x0C)
        if next_event:
            nodes.append((next_event, 0x20))
    return nodes


def _e36_alloc_event_branch(args, windows):
    pool_head = next(win[3] for win in windows if win[1] == 0x008EF7F4)
    event = struct.unpack_from("<I", pool_head)[0]
    if event == 0:
        return "new"
    event_image = next(win[3] for win in windows if win[1] == event)
    owner = struct.unpack_from("<I", event_image, 0x14)[0]
    return "pop" if owner == 0x008EF7F4 else "detached"


def _e37_test_4b20_direct_fuzz(_mu, heap, rng):
    event = heap + 0x10000
    data = bytearray(rng.getrandbits(8) for _ in range(0xE8))
    flags = rng.choice((0, 0x2000, 0x4000, 0x6000)) | (rng.getrandbits(16) & ~0x6000)
    struct.pack_into("<I", data, 0xE4, flags)
    return [], [(event, bytes(data))], event


def _e37_refresh_4a20_direct_fuzz(_mu, heap, rng):
    event = heap + 0x10000
    bodies = (heap + 0x11000, heap + 0x13000)
    grids = (heap + 0x12000, heap + 0x14000)
    data = bytearray(rng.getrandbits(8) for _ in range(0xE8))
    writes = []
    sides = ((0x80, 0x84, 0x88, 0x98), (0xAC, 0xB0, 0xB4, 0xA0))
    for (id0, id1, time, body_offset), body, grid in zip(sides, bodies, grids):
        mode = rng.randrange(4)
        # Include values on both sides of the strict 0.5 comparison and its equality case.
        time_bits = rng.choice((0x3EFFFFFE, 0x3F000000, 0x3F000001,
                                rng.randrange(0x00000000, 0x3F800001)))
        selected_id = 0 if mode != 0 else rng.choice((1, 0x100, 0xFFFFFFFF))
        other_id = rng.getrandbits(32)
        if time_bits < 0x3F000000:
            first_id, second_id = selected_id, other_id
        else:
            first_id, second_id = other_id, selected_id
        struct.pack_into("<3I", data, id0, first_id, second_id, time_bits)

        if mode == 1:
            body_ptr = 0
        else:
            body_ptr = body
            body_image = bytearray(rng.getrandbits(8) for _ in range(0x68))
            active = mode == 3
            flags = rng.getrandbits(8) & ~2 | (2 if active else 0)
            struct.pack_into("<B", body_image, 0x60, flags)
            struct.pack_into("<I", body_image, 0x64, grid if active else 0)
            writes.append((body, bytes(body_image)))
            if active:
                tag = rng.randrange(256)
                writes.append((grid + 0x68F, bytes((tag,))))
        struct.pack_into("<I", data, body_offset, body_ptr)
    writes.insert(0, (event, bytes(data)))
    return [], writes, event


def _e37_refresh_4a20_nodes(mu, args):
    event = args[0]
    event_image = bytes(mu.mem_read(event, 0xE8))
    nodes = []
    for body_offset in (0x98, 0xA0):
        body = struct.unpack_from("<I", event_image, body_offset)[0]
        if body:
            body_image = bytes(mu.mem_read(body, 0x68))
            nodes.append((body, 0x68))
            grid = struct.unpack_from("<I", body_image, 0x64)[0]
            if grid and body_image[0x60] & 2:
                nodes.append((grid + 0x68F, 1))
    return nodes


def _e37_refresh_4a20_branch(args, windows):
    event, = args
    event_image = next(window[3] for window in windows if window[1] == event)
    refreshed = []
    for id0, id1, time, body_offset in ((0x80, 0x84, 0x88, 0x98),
                                        (0xAC, 0xB0, 0xB4, 0xA0)):
        seconds = struct.unpack_from("<f", event_image, time)[0]
        selected_id = struct.unpack_from("<I", event_image, id0 if seconds < 0.5 else id1)[0]
        body = struct.unpack_from("<I", event_image, body_offset)[0]
        body_window = next((window[3] for window in windows if window[1] == body), b"")
        refreshed.append(bool(selected_id == 0 and body and len(body_window) >= 0x68 and
                              body_window[0x60] & 2))
    return ("both" if all(refreshed) else "first" if refreshed[0] else
            "second" if refreshed[1] else "none")


def _e37_get94_direct_fuzz(_mu, heap, rng):
    event = heap + 0x10000
    data = bytearray(rng.getrandbits(8) for _ in range(0x98))
    finite = rng.getrandbits(32) & 0xFF7FFFFF
    struct.pack_into("<I", data, 0x94, rng.choice((0, 0x80000000, 0x3F800000,
                                                   0xBF800000, finite)))
    return [], [(event, bytes(data))], event


def _e37_get_f7f0_direct_fuzz(_mu, _heap, rng):
    value = rng.choice((0, 1, 0x7FFFFFFF, 0xFFFFFFFF, rng.getrandbits(32)))
    return [], [(0x008EF7F0, struct.pack("<I", value))], 0


def _e37_get_f7f0_nodes(_mu, _args):
    return [(0x008EF7F0, 4)]


def _e37_scalar_getter_direct_fuzz(_mu, _heap, _rng):
    return [], [], 0


def _e37_cmp_int_direct_fuzz(_mu, heap, rng):
    a, b = heap + 0x10000, heap + 0x11000
    values = (0, 1, 2, 0x7FFFFFFF, 0x80000000, 0xFFFFFFFE, 0xFFFFFFFF,
              rng.getrandbits(32))
    x, y = rng.choice(values), rng.choice(values)
    left = struct.pack("<II", x, rng.getrandbits(32))
    right = struct.pack("<II", y, rng.getrandbits(32))
    return [a, b], [(a, left), (b, right)], 0


def _e37_cmp_int_branch(args, windows):
    a, b = args
    left = next(window[3] for window in windows if window[1] == a)
    right = next(window[3] for window in windows if window[1] == b)
    x, y = struct.unpack_from("<I", left)[0], struct.unpack_from("<I", right)[0]
    return "lt" if x < y else "gt" if x > y else "eq"


def _e37_cmp_float_direct_fuzz(_mu, heap, rng):
    a, b = heap + 0x10000, heap + 0x11000
    values = (0x00000000, 0x80000000, 0x3F800000, 0x40000000,
              0xBF800000, 0x7F800000, 0xFF800000, 0x7FC00000, rng.getrandbits(32))
    x, y = rng.choice(values), rng.choice(values)
    left = struct.pack("<II", rng.getrandbits(32), x)
    right = struct.pack("<II", rng.getrandbits(32), y)
    return [a, b], [(a, left), (b, right)], 0


def _e37_cmp_float_branch(args, windows):
    a, b = args
    left = next(window[3] for window in windows if window[1] == a)
    right = next(window[3] for window in windows if window[1] == b)
    x = struct.unpack_from("<f", left, 4)[0]
    y = struct.unpack_from("<f", right, 4)[0]
    if x != x or y != y:
        return "unordered"
    return "lt" if x < y else "gt" if x > y else "eq"


def _e37_find_int_direct_fuzz(_mu, heap, rng):
    base = heap + 0x10000
    count = rng.choice((0, 1, 2, 3, 4, 8, 17))
    # Keep keys sorted, while repeating some values so the search's duplicate
    # selection is covered as well as ordinary hits and misses.
    keys = []
    for _ in range(count):
        candidate = rng.choice((0, 1, 2, 0x7FFFFFFF, 0x80000000,
                                0xFFFFFFFE, 0xFFFFFFFF, rng.getrandbits(32)))
        if keys and rng.randrange(4) == 0:
            candidate = keys[-1]
        keys.append(candidate)
    keys.sort()
    data = b"".join(struct.pack("<II", key, rng.getrandbits(32)) for key in keys)
    if not data:
        data = bytes(8)
    if count and rng.randrange(2):
        key = rng.choice(keys)
    elif not count:
        key = rng.getrandbits(32)
    else:
        # Select a likely miss, including values outside the range.
        candidates = [0, 0xFFFFFFFF]
        candidates.extend(value for value in (keys[0] - 1, keys[-1] + 1)
                          if 0 <= value <= 0xFFFFFFFF)
        key = rng.choice(candidates)
    return [base, count, key], [(base, data)], 0


def _e37_init_70_direct_fuzz(_mu, heap, rng):
    obj, out = heap + 0x10000, heap + 0x11000
    source = bytearray(rng.getrandbits(8) for _ in range(0x80))
    source[0x70:0x80] = bytes(rng.getrandbits(8) for _ in range(16))
    destination = bytes(rng.getrandbits(8) for _ in range(16))
    return [out], [(obj, bytes(source)), (out, destination)], obj


def _e37_plane_scale_direct_fuzz(_mu, heap, rng):
    event, vector = heap + 0x10000, heap + 0x11000
    event_data = bytes(rng.getrandbits(8) for _ in range(0xD0))
    vector_data = bytes(rng.getrandbits(8) for _ in range(16))
    return [vector], [(event, event_data), (vector, vector_data)], event


def _e37_elem_getter_direct_fuzz(_mu, heap, rng):
    elem = heap + 0x10000
    data = bytes(rng.getrandbits(8) for _ in range(0x690))
    return [], [(elem, data)], elem


def _e37_elem_index_direct_fuzz(_mu, heap, rng):
    elem = heap + 0x10000
    data = bytearray(rng.getrandbits(8) for _ in range(0x690))
    struct.pack_into("<I", data, 0x410, rng.getrandbits(32))
    index = rng.choice((-2147483648, -1000, -257, -256, -255, -35, -1,
                        0, 1, 3, 4, 25, 255, 256, 0x7fffffff))
    return [index & 0xFFFFFFFF], [(elem, bytes(data))], elem


def _e37_elem_set_direct_fuzz(_mu, heap, rng):
    elem = heap + 0x10000
    data = bytes(rng.getrandbits(8) for _ in range(0x690))
    value = rng.choice((0, 1, 2, 0x100, 0x7fffffff, 0x80000000, 0xffffffff))
    return [value], [(elem, data)], elem


def _e37_elem_small_getter_direct_fuzz(_mu, heap, rng):
    elem = heap + 0x10000
    data = bytes(rng.getrandbits(8) for _ in range(0x30))
    return [], [(elem, data)], elem


def _e37_elem_small_set_direct_fuzz(_mu, heap, rng):
    elem = heap + 0x10000
    data = bytes(rng.getrandbits(8) for _ in range(0x30))
    value = rng.choice((0, 1, 2, 0x100, 0x7fffffff, 0x80000000, 0xffffffff, rng.getrandbits(32)))
    return [value], [(elem, data)], elem


def _e37_elem_valid_5990_direct_fuzz(_mu, heap, rng):
    elem = heap + 0x10000
    data = bytearray(rng.getrandbits(8) for _ in range(0x30))
    mode = rng.randrange(3)
    if mode == 0:
        struct.pack_into("<H", data, 0x0C, rng.choice((1, 2, 0x8000, 0xffff)))
    else:
        struct.pack_into("<H", data, 0x0C, 0)
        struct.pack_into("<I", data, 0x2C, 0 if mode == 1 else rng.choice((1, 0x80000000, 0xffffffff)))
    return [], [(elem, bytes(data))], elem


def _e37_elem_global_nodes(_mu, _args):
    return [(0x008F7010, 4)]


def _e37_elem_global_direct_fuzz(_mu, _heap, rng):
    value = rng.choice((0, 1, 0x7fffffff, 0xffffffff, rng.getrandbits(32)))
    return [], [(0x008F7010, struct.pack("<I", value))], 0


def _e37_nop_direct_fuzz(_mu, _heap, _rng):
    return [], [], 0


def _e37_compose_9_direct_fuzz(_mu, heap, rng):
    base = heap + 0x10000
    pointers = [base + i * 0x10 for i in range(9)]
    writes = []
    for pointer in pointers:
        values = [rng.uniform(-8.0, 8.0) for _ in range(4)]
        writes.append((pointer, struct.pack("<4f", *values)))
    return pointers, writes, 0


def _e37_tri_5a90_direct_fuzz(_mu, heap, rng):
    base = heap + 0x10000
    pointers = [base + i * 0x40 for i in range(6)]
    mode = rng.randrange(11)
    a = rng.choice((0.5, 1.0, 2.0, 4.0))
    b = rng.choice((0.5, 1.0, 2.0, 4.0))
    d = rng.choice((0.5, 1.0, 2.0, 4.0))
    if mode == 1:
        a, b, d = 1.0, 1.0, 5.0e-7

    p3 = tuple(float(rng.randrange(-4, 5)) for _ in range(3))
    p2 = (0.0, 0.0, d if mode == 2 else -d)
    p4 = (p3[0] + a, p3[1], p3[2])
    p5 = (p3[0], p3[1] + b, p3[2])
    x, y = rng.uniform(0.1, 0.4) * a, rng.uniform(0.1, 0.4) * b
    if mode == 3:
        x = -0.25 * a
    elif mode == 4:
        x = 1.25 * a
    elif mode == 5:
        x = 0.0
    elif mode == 6:
        x, y = a, 0.0
    elif mode == 7:
        y = -0.25 * b
    elif mode == 8:
        x, y = 0.75 * a, 0.5 * b
    elif mode == 9:
        x, y = 0.5 * a, 0.5 * b
    z = rng.uniform(-2.0, 2.0)
    p1 = (p3[0] + x, p3[1] + y, p3[2] + z)
    writes = [
        (pointers[0], struct.pack("<4f", *p1, rng.uniform(-1.0, 1.0))),
        (pointers[1], struct.pack("<4f", *p2, rng.uniform(-1.0, 1.0))),
        (pointers[2], struct.pack("<4f", *p3, rng.uniform(-1.0, 1.0))),
        (pointers[3], struct.pack("<4f", *p4, rng.uniform(-1.0, 1.0))),
        (pointers[4], struct.pack("<4f", *p5, rng.uniform(-1.0, 1.0))),
        (pointers[5], bytes(rng.getrandbits(8) for _ in range(0x20))),
    ]
    return pointers, writes, 0


def _e37_run_56c0_direct_fuzz(_mu, heap, rng):
    base = heap + 0x10000
    pointers = [base + i * 0x40 for i in range(6)]
    mode = rng.randrange(14)
    p3 = tuple(float(rng.randrange(-4, 5)) for _ in range(3))
    direction = [0.0, 0.0, -1.0]
    x, y = 0.25, 0.25
    if mode == 1:
        x = 0.0
    elif mode == 2:
        y = 0.0
    elif mode == 3:
        x, y = 0.5, 0.5
    elif mode == 4:
        x = -0.25
    elif mode == 5:
        x = 1.25
    elif mode == 6:
        y = -0.25
    elif mode == 7:
        x, y = 0.75, 0.5
    elif mode == 8:
        direction[2] = -5.0e-7
    elif mode == 9:
        direction[2] = 1.0
    elif mode == 10:
        direction = [1.0, 0.0, 0.0]
    elif mode == 11:
        x, y = rng.uniform(0.05, 0.4), rng.uniform(0.05, 0.4)
    elif mode == 12:
        x = float("nan")
    elif mode == 13:
        direction[2] = float("nan")

    p1 = (p3[0] + x, p3[1] + y, p3[2] + rng.uniform(-2.0, 2.0))
    p4 = (p3[0] + 1.0, p3[1], p3[2])
    p5 = (p3[0], p3[1] + 1.0, p3[2])
    vectors = [p1, tuple(direction), p3, p4, p5]
    writes = [
        (pointer, struct.pack("<4f", *vector, rng.uniform(-1.0, 1.0)))
        for pointer, vector in zip(pointers[:5], vectors)
    ]
    writes.append((pointers[5], struct.pack("<I", rng.getrandbits(32))))
    return pointers, writes, 0


def _e37_elem_5f20_nodes(mu, args):
    table = struct.unpack("<I", bytes(mu.mem_read(args[0] + 0x410, 4)))[0]
    return [(table + args[1] * 7, 7)]


def _e37_elem_5f20_direct_fuzz(_mu, heap, rng):
    grid, table = heap + 0x10000, heap + 0x12000
    start, end = heap + 0x13000, heap + 0x13100
    index, mode = rng.randrange(4), rng.randrange(8)
    v0, v1, v2 = (0.0, 0.0, 0.0), (1.0, 0.0, 0.0), (0.0, 1.0, 0.0)
    if mode == 6:
        v1, v2 = v2, v1
    grid_bytes = bytearray(rng.getrandbits(8) for _ in range(0x1000))
    struct.pack_into("<I", grid_bytes, 0x410, table)
    for slot, vector in enumerate((v0, v1, v2)):
        struct.pack_into("<4f", grid_bytes, 0x40 + slot * 0x10, *vector, rng.uniform(-1.0, 1.0))
    rows = bytearray(rng.getrandbits(8) for _ in range(4 * 7))
    for row in range(4):
        rows[row * 7:row * 7 + 3] = bytes((0, 1, 2))
        rows[row * 7 + 3] = 0
    if mode == 0:
        rows[index * 7 + 3] = 0xff
    x, y = (0.25, 0.25)
    z0, z1 = 1.0, -1.0
    if mode == 2:
        x, y = 1.5, 0.25
    elif mode == 3:
        z0 = 0.0
    elif mode == 4:
        z1 = 0.0
    elif mode == 5:
        z0, z1 = 2.0, 1.0
    elif mode == 7:
        x, y = 0.75, 0.25
    start_bytes = struct.pack("<4f", x, y, z0, rng.uniform(-1.0, 1.0))
    end_bytes = struct.pack("<4f", x, y, z1, rng.uniform(-1.0, 1.0))
    writes = [(grid, bytes(grid_bytes)), (table, bytes(rows)), (start, start_bytes), (end, end_bytes)]
    return [index, start, end], writes, grid


def _e38_vector_bytes(rng):
    pool = (0.0, -0.0, 0.5, -1.0, 16.0, -16.0, 1024.0, -1024.0, 1.0e20, -1.0e20)
    return struct.pack("<4f", *(rng.choice(pool) if rng.randrange(3) == 0 else rng.uniform(-100.0, 100.0)
                                  for _ in range(4)))


def _e38_scale_direct_fuzz(_mu, heap, rng):
    vector = heap + 0x10000
    factor = rng.choice((-2.0, -1.0, -0.0, 0.0, 0.5, 1.0, 2.0, rng.uniform(-8.0, 8.0)))
    return [struct.unpack("<I", struct.pack("<f", factor))[0]], [(vector, _e38_vector_bytes(rng))], vector


def _e38_scale_to_direct_fuzz(_mu, heap, rng):
    this, output = heap + 0x10000, heap + 0x11000
    out = bytes(rng.getrandbits(8) for _ in range(16))
    factor = rng.choice((-2.0, -1.0, -0.0, 0.0, 0.5, 1.0, 2.0, rng.uniform(-8.0, 8.0)))
    writes = [(this, _e38_vector_bytes(rng)), (output, out)]
    return [struct.unpack("<I", struct.pack("<f", factor))[0], output], writes, this


def _e38_normalize_direct_fuzz(_mu, heap, rng):
    vector = heap + 0x10000
    specials = (0x00000000, 0x80000000, 0x00000001, 0x007fffff,
                0x3f800000, 0xbf800000, 0x7f800000, 0xff800000,
                0x7fc00001, 0xffc12345)
    mode = rng.randrange(8)
    if mode == 0:
        lanes = (0, 0, 0)
    elif mode == 1:
        lanes = (0x80000000, 0x80000000, 0x80000000)
    elif mode == 2:
        lanes = (0x7f800000, 0, 0)
    elif mode == 3:
        lanes = (0xff800000, 0x3f800000, 0)
    elif mode == 4:
        lanes = (0x7fc00001, 0x3f800000, 0xbf800000)
    elif mode == 5:
        lanes = (0x3f800000, 0xffc12345, 0xbf800000)
    else:
        lanes = tuple(rng.choice(specials) if rng.randrange(3) == 0 else
                      struct.unpack("<I", struct.pack("<f", rng.uniform(-10.0, 10.0)))[0]
                      for _ in range(3))
    data = struct.pack("<4I", *lanes, rng.getrandbits(32))
    return [], [(vector, data)], vector


def _e38_sat_direct_fuzz(_mu, heap, rng):
    this, center, extent = heap + 0x10000, heap + 0x11000, heap + 0x12000
    o = [rng.uniform(-4.0, 4.0) for _ in range(3)]
    half = [rng.choice((0.25, 0.5, 1.0, 2.0, 4.0)) for _ in range(3)]
    c = [rng.choice((0.125, 0.25, 0.5, 1.0, 2.0)) for _ in range(3)]
    mode = rng.randrange(9)
    separation = [0.0, 0.0, 0.0]
    if mode in (1, 4, 7):
        axis = {1: 0, 4: 1, 7: 2}[mode]
        separation[axis] = half[axis] + c[axis] + (0.0 if mode == 4 else 0.5)
    elif mode in (2, 5, 8):
        axis = {2: 0, 5: 1, 8: 2}[mode]
        separation[axis] = -(half[axis] + c[axis] + 0.5)
    other_center = [o[i] + separation[i] for i in range(3)]
    if mode == 6:
        other_center[0] = float("nan")
    this_bytes = struct.pack("<6f", *o, *half)
    center_bytes = struct.pack("<3f", *other_center)
    extent_bytes = struct.pack("<3f", *c)
    return [center, extent], [(this, this_bytes), (center, center_bytes), (extent, extent_bytes)], this


def _e38_mesh_ac50_nodes(mu, args):
    base = struct.unpack("<I", bytes(mu.mem_read(args[0] + 0xA0, 4)))[0]
    return [(base, 64)]


def _e38_mesh_ac50_direct_fuzz(_mu, heap, rng):
    this, origin, direction = heap + 0x10000, heap + 0x11000, heap + 0x12000
    output, distance, base = heap + 0x13000, heap + 0x14000, heap + 0x15000
    mode = rng.randrange(12)
    x, y, z = 0.75, 0.25, 1.5
    if mode == 1:
        x, y = 0.25, 0.75
    elif mode == 2:
        x, y, z = 0.75, 0.25, -1.5
    elif mode == 3:
        x, y, z = 0.25, 0.75, -1.5
    elif mode == 4:
        x, y, z = 0.5, 0.5, 0.5
    elif mode == 5:
        x, y = 1.25, 0.25
    elif mode == 6:
        x, y = 0.25, 1.25
    elif mode == 7:
        x, y = -0.25, 0.25
    elif mode == 8:
        x, y, z = 0.1, 0.3, 0.25
    elif mode == 9:
        x, y, z = 0.3, 0.1, 0.25
    elif mode == 10:
        x, y, z = 0.25, 0.25, 0.0
    elif mode == 11:
        x, y, z = rng.uniform(0.05, 0.95), rng.uniform(0.05, 0.95), rng.uniform(-2.0, 2.0)

    mesh_origin = tuple(float(rng.randrange(-3, 4)) for _ in range(3))
    origin_values = [mesh_origin[0] + x, mesh_origin[1] + y, mesh_origin[2] + z]
    vertices = [mesh_origin,
                (mesh_origin[0] + 1.0, mesh_origin[1], mesh_origin[2]),
                (mesh_origin[0] + 1.0, mesh_origin[1] + 1.0, mesh_origin[2]),
                (mesh_origin[0], mesh_origin[1] + 1.0, mesh_origin[2])]
    mesh_bytes = bytearray(rng.getrandbits(8) for _ in range(0xC0))
    struct.pack_into("<I", mesh_bytes, 0xA0, base)
    tags = [rng.getrandbits(32) for _ in range(4)]
    struct.pack_into("<4I", mesh_bytes, 0xB0, *tags)
    points = b"".join(struct.pack("<4f", *vertex, rng.uniform(-1.0, 1.0)) for vertex in vertices)
    writes = [
        (this, bytes(mesh_bytes)),
        (origin, struct.pack("<4f", *origin_values, rng.uniform(-1.0, 1.0))),
        (direction, struct.pack("<4f", 0.0, 0.0, 1.0, rng.uniform(-1.0, 1.0))),
        (output, bytes(rng.getrandbits(8) for _ in range(16))),
        (distance, struct.pack("<f", rng.choice((-2.0, -0.0, 0.0, 2.0)))),
        (base, points),
    ]
    return [origin, direction, output, distance], writes, this


def _e38_clear_4fad10_direct_fuzz(_mu, heap, rng):
    target = heap + 0x10000
    initial = bytes(rng.getrandbits(8) for _ in range(0x804))
    return [], [(target, initial)], target


def _e38_solver_def_table_direct_fuzz(_mu, heap, rng):
    this = heap + 0x10000 + rng.randrange(0x1000 // 4) * 4
    return [], [], this


def _e38_solver_4fb720_nodes(mu, _args):
    # g_1659b70 points to the host object; its +0x24 field points at the material-manager base.
    word = lambda at: struct.unpack("<I", bytes(mu.mem_read(at, 4)))[0]
    manager_slot = word(0x01659B70) + 0x24
    return [(0x01659B70, 4), (manager_slot, 4)]


def _e38_solver_4fb720_direct_fuzz(_mu, heap, rng):
    body, output = heap + 0x10000, heap + 0x11000
    data = bytearray(rng.getrandbits(8) for _ in range(0xE8))
    mode = rng.randrange(8)
    use_primary = mode & 1
    threshold = (-1.0, 0.0, 0.49, 0.5, 0.5000000596046448, 0.75, float("nan"), float("inf"))[mode]
    low = rng.choice((0, 1, 7, 0xFF, 0x100, 0x12345678))
    high = rng.choice((0, 2, 15, 0xFE, 0x101, 0x87654321))
    offset = 0x80 if use_primary else 0xAC
    struct.pack_into("<III", data, offset, low, high,
                     struct.unpack("<I", struct.pack("<f", threshold))[0])
    flags = rng.getrandbits(32) & ~0x800
    if use_primary:
        flags |= 0x800
    struct.pack_into("<I", data, 0xE4, flags)
    writes = [(body, bytes(data)), (output, struct.pack("<I", rng.getrandbits(32)))]
    return [body, output], writes, 0


def _e38_solver_4fb720_branch(args, windows):
    body = windows[0][3]
    flags = struct.unpack_from("<I", body, 0xE4)[0]
    offset = 0x80 if flags & 0x800 else 0xAC
    threshold = struct.unpack_from("<f", body, offset + 8)[0]
    path = "primary" if flags & 0x800 else "secondary"
    return f"{path}_{'high' if threshold > 0.5 else 'low'}"


def _e38_d480_direct_fuzz(_mu, heap, rng):
    this, origin, direction = heap + 0x10000, heap + 0x11000, heap + 0x12000
    vertex0, vertex1, vertex2, output = heap + 0x13000, heap + 0x14000, heap + 0x15000, heap + 0x16000
    base = [rng.uniform(-4.0, 4.0) for _ in range(3)]
    direction_values = [0.0, 0.0, 1.0]
    mode = rng.randrange(12)
    x, y = 0.25, 0.25
    if mode == 1:
        x, y = 0.0, 0.25
    elif mode == 2:
        x, y = 0.25, 0.0
    elif mode == 3:
        x, y = 0.5, 0.5
    elif mode == 4:
        x = -0.25
    elif mode == 5:
        y = -0.25
    elif mode == 6:
        x = 1.25
    elif mode == 7:
        x, y = 0.75, 0.5
    elif mode == 8:
        direction_values = [1.0, 0.0, 0.0]
    elif mode == 9:
        direction_values = [0.0, 0.0, -1.0]
    elif mode == 10:
        x, y = rng.uniform(0.05, 0.4), rng.uniform(0.05, 0.4)
    elif mode == 11:
        x = float("nan")
    origin_values = [base[0] + x, base[1] + y, base[2] + rng.uniform(-2.0, 2.0)]
    vertices = [base, [base[0] + 1.0, base[1], base[2]], [base[0], base[1] + 1.0, base[2]]]
    inputs = [this, origin, direction, vertex0, vertex1, vertex2, output]
    values = [[rng.uniform(-1.0, 1.0) for _ in range(4)],
              origin_values + [rng.uniform(-1.0, 1.0)],
              direction_values + [0.0],
              vertices[0] + [0.0], vertices[1] + [0.0], vertices[2] + [0.0]]
    writes = [(ptr, struct.pack("<4f", *lane)) for ptr, lane in zip(inputs[:6], values)]
    writes.append((output, bytes(rng.getrandbits(8) for _ in range(44))))
    return inputs[1:], writes, this


def _e38_binary_vec_direct_fuzz(_mu, heap, rng):
    this, other, output = heap + 0x10000, heap + 0x11000, heap + 0x12000
    writes = [(this, _e38_vector_bytes(rng)), (other, _e38_vector_bytes(rng)),
              (output, bytes(rng.getrandbits(8) for _ in range(16)))]
    return [other, output], writes, this


def _e38_add_sub_direct_fuzz(_mu, heap, rng):
    this, b, c, output = heap + 0x10000, heap + 0x11000, heap + 0x12000, heap + 0x13000
    writes = [(ptr, _e38_vector_bytes(rng)) for ptr in (this, b, c)]
    writes.append((output, bytes(rng.getrandbits(8) for _ in range(16))))
    return [b, c, output], writes, this


def _e38_dot_direct_fuzz(_mu, heap, rng):
    this, other = heap + 0x10000, heap + 0x11000
    writes = [(this, _e38_vector_bytes(rng)), (other, _e38_vector_bytes(rng))]
    return [other], writes, this


def _e38_three_vecs_direct_fuzz(_mu, heap, rng):
    this, b, output, d = (heap + 0x10000, heap + 0x11000,
                          heap + 0x12000, heap + 0x13000)
    writes = [(ptr, _e38_vector_bytes(rng)) for ptr in (this, b, d)]
    writes.append((output, bytes(rng.getrandbits(8) for _ in range(16))))
    return [b, output, d], writes, this


def _e37_copy_80_direct_fuzz(_mu, heap, rng):
    event, output = heap + 0x10000, heap + 0x11000
    data = bytes(rng.getrandbits(8) for _ in range(0x8C))
    out = bytes(rng.getrandbits(8) for _ in range(12))
    return [output], [(event, data), (output, out)], event


def _e37_cmp_15_direct_fuzz(_mu, heap, rng):
    event, other = heap + 0x10000, heap + 0x11000
    lanes = (0, 1, 2, 4, 5, 6, 7, 13, 14)
    finite = (0x00000000, 0x80000000, 0x3F800000, 0xBF800000, 0x40000000,
              0x7F800000, 0xFF800000)
    left = bytearray(rng.getrandbits(8) for _ in range(0x3C))
    right = bytearray(rng.getrandbits(8) for _ in range(0x3C))
    mode = rng.randrange(9)
    struct.pack_into("<I", left, 0x0C, mode)
    struct.pack_into("<I", right, 0x0C, mode)
    for lane in lanes:
        bits = rng.choice(finite)
        other_bits = bits
        if mode == 0 and lane == lanes[0] and bits == 0:
            other_bits = 0x80000000
        struct.pack_into("<I", left, lane * 4, bits)
        struct.pack_into("<I", right, lane * 4, other_bits)
    if mode == 1:
        lane = rng.choice(lanes)
        old = struct.unpack_from("<I", left, lane * 4)[0]
        if old in (0, 0x80000000):
            different = 0x3F800000
        else:
            different = next(bits for bits in finite if bits != old)
        struct.pack_into("<I", right, lane * 4, different)
    elif mode == 2:
        lane = 14
        struct.pack_into("<I", left, lane * 4, 0x7FC00000)
        struct.pack_into("<I", right, lane * 4, 0x7FC00000)
    elif mode == 8:
        lane = rng.choice(lanes[:-1])
        struct.pack_into("<I", left, lane * 4, 0x7FC00000)
        struct.pack_into("<I", right, lane * 4, 0x7FC00000)
    elif mode in (4, 5, 6, 7):
        lane = 14 if mode == 5 else rng.choice(lanes)
        left_nan = 0x7FC00001 if mode != 6 else 0x7F800001
        right_nan = 0x7FC00002
        left_value = 0x3F800000
        right_value = 0x40000000
        if mode == 4:
            left_value, right_value = left_nan, right_value
        elif mode == 5:
            left_value, right_value = left_nan, right_nan
        elif mode == 6:
            left_value, right_value = left_nan, right_value
        else:
            left_value, right_value = left_value, right_nan
        struct.pack_into("<I", left, lane * 4, left_value)
        struct.pack_into("<I", right, lane * 4, right_value)
    return [other], [(event, bytes(left)), (other, bytes(right))], event


def _e37_word_direct_fuzz(_mu, heap, rng, length):
    obj = heap + 0x10000
    data = bytes(rng.getrandbits(8) for _ in range(length))
    return [], [(obj, data)], obj


def _e37_zero3_direct_fuzz(mu, heap, rng):
    return _e37_word_direct_fuzz(mu, heap, rng, 12)


def _e37_first_direct_fuzz(mu, heap, rng):
    return _e37_word_direct_fuzz(mu, heap, rng, 4)


def _e37_at8_direct_fuzz(mu, heap, rng):
    return _e37_word_direct_fuzz(mu, heap, rng, 12)


def _e37_zero4_direct_fuzz(mu, heap, rng):
    return _e37_word_direct_fuzz(mu, heap, rng, 16)


def _e37_list_insert_nodes(mu, args):
    first = _read_u32(mu, args[0])
    return [(first, 0x14)] if first else []


def _e37_list_insert_direct_fuzz(_mu, heap, rng):
    head, node, first = heap + 0x10000, heap + 0x11000, heap + 0x12000
    existing = rng.randrange(2) != 0
    head_image = struct.pack("<3I", first if existing else 0,
                             rng.getrandbits(32), rng.choice((0, 1, 4, 0xFFFFFFFF)))
    node_image = bytes(rng.getrandbits(8) for _ in range(0x18))
    writes = [(head, head_image), (node, node_image)]
    if existing:
        writes.append((first, bytes(rng.getrandbits(8) for _ in range(0x14))))
    return [node], writes, head


def _e37_unlink_direct_fuzz(_mu, heap, rng):
    node, next_node, prev_node, head = (heap + 0x10000, heap + 0x11000,
                                        heap + 0x12000, heap + 0x13000)
    mode = rng.randrange(6)
    next_ptr = next_node if mode in (1, 3, 4) else 0
    prev_ptr = prev_node if mode in (2, 3, 5) else 0
    head_ptr = head if mode in (0, 1, 2, 3) else 0
    node_image = bytearray(struct.pack("<3I", next_ptr, prev_ptr, head_ptr))
    writes = [(node, bytes(node_image))]
    if next_ptr:
        image = bytearray(rng.getrandbits(8) for _ in range(0x14))
        struct.pack_into("<I", image, 0x10, node)
        writes.append((next_node, bytes(image)))
    if prev_ptr:
        image = bytearray(rng.getrandbits(8) for _ in range(0x14))
        struct.pack_into("<I", image, 0x0C, node)
        writes.append((prev_node, bytes(image)))
    if head_ptr:
        first = node if not prev_ptr else rng.getrandbits(32)
        last = node if not next_ptr else rng.getrandbits(32)
        writes.append((head, struct.pack("<2I", first, last)))
    return [], writes, node


def _e37_unlink_nodes(mu, args):
    node = args[0]
    next_node, prev_node, head = (_read_u32(mu, node + offset) for offset in (0, 4, 8))
    return [(at, length) for at, length in ((next_node, 0x14), (prev_node, 0x14), (head, 8)) if at]


def _e37_rate_direct_fuzz(_mu, heap, rng):
    obj, link = heap + 0x10000, heap + 0x11000
    active = rng.randrange(2) != 0
    value = rng.choice((0x00000000, 0x80000000, 0x3F800000, 0xBF800000,
                        0x40200000, 0xC0500000, 0x3DCCCCCD))
    image = bytearray(0x10)
    link_image = bytearray(0x98)
    if active:
        struct.pack_into("<I", image, 4, link)
        struct.pack_into("<I", image, 0x0C, 1)
        struct.pack_into("<I", link_image, 0x94, value)
    return [], [(obj, bytes(image)), (link, bytes(link_image))], obj


def _e37_rate_nodes(mu, args):
    obj = args[0]
    link = _read_u32(mu, obj + 4)
    return [(link, 0x98)] if _read_u32(mu, obj + 0x0C) and link else []


def _e37_push_front_direct_fuzz(_mu, heap, rng):
    node, old_head, old_tail = heap + 0x10000, heap + 0x11000, heap + 0x12000
    use_old_head = rng.randrange(2) != 0
    old_head_ptr = old_head if use_old_head else 0
    old_tail_ptr = old_tail if not use_old_head else rng.getrandbits(32)
    node_image = bytes(rng.getrandbits(8) for _ in range(0x18))
    writes = [(node, node_image),
              (0x008EF7F4, struct.pack("<3I", old_head_ptr, old_tail_ptr,
                                       rng.choice((0, 1, 0x7FFFFFFF, 0xFFFFFFFF))))]
    if use_old_head:
        old_head_image = bytearray(rng.getrandbits(8) for _ in range(0x14))
        writes.append((old_head, bytes(old_head_image)))
    return [node], writes, 0


def _e37_push_front_nodes(mu, args):
    old_head = _read_u32(mu, 0x008EF7F4)
    nodes = [(0x008EF7F4, 12)]
    if old_head:
        nodes.append((old_head, 0x14))
    return nodes


def _e37_push_front_branch(args, windows):
    global_window = next(window for window in windows if window[1] == 0x008EF7F4)
    old_head = struct.unpack_from("<I", global_window[3])[0]
    return "empty" if old_head == 0 else "nonempty"


def _e37_event_dtor_direct_fuzz(_mu, heap, rng):
    event, owner, next_node, prev_node = (heap + 0x10000, heap + 0x11000,
                                          heap + 0x12000, heap + 0x13000)
    mode = rng.randrange(5)
    owner_ptr = owner if mode else 0
    next_ptr = next_node if mode in (2, 4) else 0
    prev_ptr = prev_node if mode in (3, 4) else 0
    image = bytearray(rng.getrandbits(8) for _ in range(0x24))
    struct.pack_into("<3I", image, 0x0C, next_ptr, prev_ptr, owner_ptr)
    writes = [(event, bytes(image)),
              (0x008EF7F0, struct.pack("<I", rng.choice((0, 1, 0x7FFFFFFF, 0xFFFFFFFF))))]
    if owner_ptr:
        first = prev_node if prev_ptr else event + 0x0C
        last = next_node if next_ptr else event + 0x0C
        owner_image = bytearray(rng.getrandbits(8) for _ in range(0x0C))
        struct.pack_into("<3I", owner_image, 0, first, last,
                         rng.choice((0, 1, 2, 0xFFFFFFFF)))
        writes.append((owner, bytes(owner_image)))
    if next_ptr:
        next_image = bytearray(rng.getrandbits(8) for _ in range(0x14))
        struct.pack_into("<I", next_image, 0x10, event + 0x0C)
        writes.append((next_node, bytes(next_image)))
    if prev_ptr:
        prev_image = bytearray(rng.getrandbits(8) for _ in range(0x14))
        struct.pack_into("<I", prev_image, 0x0C, event + 0x0C)
        writes.append((prev_node, bytes(prev_image)))
    return [], writes, event


def _e37_event_dtor_nodes(mu, args):
    event = args[0]
    owner = _read_u32(mu, event + 0x14)
    nodes = [(0x008EF7F0, 4)]
    if owner:
        nodes.append((owner, 0x0C))
        next_node = _read_u32(mu, event + 0x0C)
        prev_node = _read_u32(mu, event + 0x10)
        nodes.extend((at, 0x14) for at in (next_node, prev_node) if at)
    return nodes


def _e37_event_dtor_branch(args, windows):
    image = windows[0][3]
    owner = struct.unpack_from("<I", image, 0x14)[0]
    if not owner:
        return "ownerless"
    next_node, prev_node = struct.unpack_from("<2I", image, 0x0C)
    if not next_node and not prev_node:
        return "alone"
    if next_node and not prev_node:
        return "next-only"
    if prev_node and not next_node:
        return "previous-only"
    return "both"


def _e37_event_vf1cx_direct_fuzz(_mu, heap, rng):
    event, owner, next_node, prev_node = (heap + 0x10000, heap + 0x11000,
                                          heap + 0x12000, heap + 0x13000)
    old_free_head, old_free_tail = heap + 0x14000, heap + 0x15000
    mode = rng.randrange(5)
    owner_ptr = owner if mode else 0
    next_ptr = next_node if mode in (2, 4) else 0
    prev_ptr = prev_node if mode in (3, 4) else 0
    image = bytearray(rng.getrandbits(8) for _ in range(0x24))
    struct.pack_into("<3I", image, 0x0C, next_ptr, prev_ptr, owner_ptr)
    use_free_head = rng.randrange(2) != 0
    free_head_ptr = old_free_head if use_free_head else 0
    free_tail_ptr = rng.getrandbits(32) if use_free_head else old_free_tail
    flags = rng.choice((0, 1, 2, 3, 0xFFFFFFFE, 0xFFFFFFFF))
    writes = [(event, bytes(image)),
              (0x008EF7F0, struct.pack("<I", rng.choice((0, 1, 0x7FFFFFFF, 0xFFFFFFFF)))),
              (0x008EF7F4, struct.pack("<3I", free_head_ptr, free_tail_ptr,
                                       rng.choice((0, 1, 2, 0xFFFFFFFF))))]
    if owner_ptr:
        first = prev_node if prev_ptr else event + 0x0C
        last = next_node if next_ptr else event + 0x0C
        owner_image = bytearray(rng.getrandbits(8) for _ in range(0x0C))
        struct.pack_into("<3I", owner_image, 0, first, last,
                         rng.choice((0, 1, 2, 0xFFFFFFFF)))
        writes.append((owner, bytes(owner_image)))
    if next_ptr:
        next_image = bytearray(rng.getrandbits(8) for _ in range(0x14))
        struct.pack_into("<I", next_image, 0x10, event + 0x0C)
        writes.append((next_node, bytes(next_image)))
    if prev_ptr:
        prev_image = bytearray(rng.getrandbits(8) for _ in range(0x14))
        struct.pack_into("<I", prev_image, 0x0C, event + 0x0C)
        writes.append((prev_node, bytes(prev_image)))
    if free_head_ptr:
        free_image = bytearray(rng.getrandbits(8) for _ in range(0x14))
        writes.append((old_free_head, bytes(free_image)))
    return [flags], writes, event


def _e37_event_vf1cx_nodes(mu, args):
    event = args[0]
    owner = _read_u32(mu, event + 0x14)
    free_head = _read_u32(mu, 0x008EF7F4)
    nodes = [(0x008EF7F0, 4), (0x008EF7F4, 12)]
    if owner:
        nodes.append((owner, 0x0C))
        next_node = _read_u32(mu, event + 0x0C)
        prev_node = _read_u32(mu, event + 0x10)
        nodes.extend((at, 0x14) for at in (next_node, prev_node) if at)
    if free_head:
        nodes.append((free_head, 0x14))
    return nodes


def _e37_event_vf1cx_branch(args, windows):
    event_image = windows[0][3]
    owner = struct.unpack_from("<I", event_image, 0x14)[0]
    next_node, prev_node = struct.unpack_from("<2I", event_image, 0x0C)
    free_window = next(window for window in windows if window[1] == 0x008EF7F4)
    free_head = struct.unpack_from("<I", free_window[3])[0]
    if not owner:
        owner_shape = "ownerless"
    elif not next_node and not prev_node:
        owner_shape = "alone"
    elif next_node and not prev_node:
        owner_shape = "next"
    elif prev_node and not next_node:
        owner_shape = "previous"
    else:
        owner_shape = "both"
    action = "pool" if args[1] & 1 else "no-pool"
    pool_shape = "empty-pool" if free_head == 0 else "linked-pool"
    return f"{action}-{owner_shape}-{pool_shape}"


def _e37_drain_direct_fuzz(_mu, heap, rng):
    head, free_head, free_tail = heap + 0x10000, heap + 0x15000, heap + 0x16000
    count = rng.choice((1, 1, 2, 2, 3, 4))
    events = [heap + 0x11000 + i * 0x1000 for i in range(count)]
    writes = []
    for i, event in enumerate(events):
        image = bytearray(rng.getrandbits(8) for _ in range(0x24))
        next_node = events[i + 1] if i + 1 < count else 0
        prev_node = events[i - 1] if i else 0
        struct.pack_into("<I", image, 0, 0x00739558)
        struct.pack_into("<3I", image, 0x0C, next_node, prev_node, head)
        writes.append((event, bytes(image)))
    head_image = bytearray(rng.getrandbits(8) for _ in range(0x0C))
    struct.pack_into("<3I", head_image, 0, events[0], events[-1], count)
    use_free_head = rng.randrange(2) != 0
    free_head_ptr = free_head if use_free_head else 0
    free_tail_ptr = rng.getrandbits(32) if use_free_head else free_tail
    flags = rng.choice((0, 0, 1, 1, 2, 3, 0xFFFFFFFF))
    writes.extend(((head, bytes(head_image)),
                   (0x008EF7F0, struct.pack("<I", rng.choice((0, 1, 2, 0xFFFFFFFF)))),
                   (0x008EF7F4, struct.pack("<3I", free_head_ptr, free_tail_ptr,
                                            rng.choice((0, 1, 2, 0xFFFFFFFF))))))
    if free_head_ptr:
        free_image = bytearray(rng.getrandbits(8) for _ in range(0x14))
        writes.append((free_head, bytes(free_image)))
    return [flags], writes, head


def _e37_drain_4f70_direct_fuzz(_mu, heap, rng):
    words, writes, head = _e37_drain_direct_fuzz(_mu, heap, rng)
    writes.append((head + 0x0C, struct.pack("<I", words[0])))
    return [], writes, head


def _e37_drain_vf08x_direct_fuzz(_mu, heap, rng):
    _, writes, head = _e37_drain_direct_fuzz(_mu, heap, rng)
    return [], writes, head - 4


def _e37_drain_nodes(mu, args):
    head = args[0]
    count = _read_u32(mu, head + 8)
    node = _read_u32(mu, head)
    nodes = [(0x008EF7F0, 4), (0x008EF7F4, 12)]
    seen = set()
    for _ in range(count):
        if not node or node in seen:
            break
        seen.add(node)
        nodes.append((node, 0x24))
        node = _read_u32(mu, node + 0x0C)
    free_head = _read_u32(mu, 0x008EF7F4)
    if free_head:
        nodes.append((free_head, 0x14))
    return nodes


def _e37_drain_vf08x_nodes(mu, args):
    return _e37_drain_nodes(mu, (args[0] + 4,))


def _e37_drain_branch(args, windows, head_offset=0):
    head_image = windows[0][3]
    count = struct.unpack_from("<I", head_image, head_offset + 8)[0]
    free_window = next(window for window in windows if window[1] == 0x008EF7F4)
    free_head = struct.unpack_from("<I", free_window[3])[0]
    action = "pool" if args[1] != 0 else "unlink"
    pool = "linked-pool" if free_head else "empty-pool"
    return f"{action}-{count}-{pool}"


def _e37_drain_4f70_branch(args, windows):
    flags = struct.unpack_from("<I", windows[0][3], 0x0C)[0]
    return _e37_drain_branch((args[0], flags), windows)


def _e37_drain_vf08x_branch(args, windows):
    return _e37_drain_branch((args[0] + 4, 1), windows, 4)


def _e37_release_owner_direct_fuzz(_mu, heap, rng):
    node, owner, next_node, prev_node = (heap + 0x10000, heap + 0x11000,
                                         heap + 0x12000, heap + 0x13000)
    mode = rng.randrange(5)
    owner_ptr = owner if mode else 0
    next_ptr = next_node if mode in (2, 4) else 0
    prev_ptr = prev_node if mode in (3, 4) else 0
    writes = [(node, struct.pack("<3I", next_ptr, prev_ptr, owner_ptr))]
    if owner_ptr:
        first = node if not prev_ptr else rng.getrandbits(32)
        last = node if not next_ptr else rng.getrandbits(32)
        count = rng.choice((0, 1, 2, 0xFFFFFFFF))
        writes.append((owner, struct.pack("<3I", first, last, count)))
    if next_ptr:
        image = bytearray(rng.getrandbits(8) for _ in range(0x14))
        struct.pack_into("<I", image, 0x10, node)
        writes.append((next_node, bytes(image)))
    if prev_ptr:
        image = bytearray(rng.getrandbits(8) for _ in range(0x14))
        struct.pack_into("<I", image, 0x0C, node)
        writes.append((prev_node, bytes(image)))
    return [], writes, node


def _e37_release_owner_nodes(mu, args):
    node = args[0]
    owner = _read_u32(mu, node + 8)
    if not owner:
        return []
    next_node, prev_node = _read_u32(mu, node), _read_u32(mu, node + 4)
    return [(owner, 12)] + [(at, 0x14) for at in (next_node, prev_node) if at]


def _e37_release_direct_fuzz(_mu, heap, rng):
    head, node, next_node, prev_node = (heap + 0x10000, heap + 0x11000,
                                        heap + 0x12000, heap + 0x13000)
    mode = rng.randrange(5)
    matching = mode != 0
    next_ptr = next_node if mode in (2, 4) else 0
    prev_ptr = prev_node if mode in (3, 4) else 0
    node_owner = head if matching else rng.getrandbits(32)
    count = rng.choice((0, 1, 2, 0xFFFFFFFF))
    first = node if matching and not prev_ptr else rng.getrandbits(32)
    last = node if matching and not next_ptr else rng.getrandbits(32)
    writes = [(head, struct.pack("<3I", first, last, count)),
              (node, struct.pack("<3I", next_ptr, prev_ptr, node_owner))]
    if next_ptr:
        image = bytearray(rng.getrandbits(8) for _ in range(0x14))
        struct.pack_into("<I", image, 0x10, node)
        writes.append((next_node, bytes(image)))
    if prev_ptr:
        image = bytearray(rng.getrandbits(8) for _ in range(0x14))
        struct.pack_into("<I", image, 0x0C, node)
        writes.append((prev_node, bytes(image)))
    return [node], writes, head


def _e37_release_nodes(mu, args):
    head, node = args[0], args[1]
    if _read_u32(mu, node + 8) != head:
        return []
    next_node, prev_node = _read_u32(mu, node), _read_u32(mu, node + 4)
    return [(at, 0x14) for at in (next_node, prev_node) if at]


def _e37_set1c_direct_fuzz(_mu, heap, rng):
    obj = heap + 0x10000
    image = bytes(rng.getrandbits(8) for _ in range(0x20))
    return [rng.getrandbits(32)], [(obj, image)], obj


def _e37_vec16_minmax_direct_fuzz(_mu, heap, rng):
    obj, other = heap + 0x10000, heap + 0x11000
    mode = rng.randrange(8)
    if mode == 0:
        left = (0x40400000, 0xC0400000, 0x00000000, 0x80000000)
        right = (0xC0400000, 0x40400000, 0x80000000, 0x00000000)
    elif mode == 1:
        left = (0x3F800000, 0xBF800000, 0x00000000, 0x80000000)
        right = left
    elif mode == 2:
        left = (0x7FC12345, 0x7F812345, 0x7FC54321, 0x7F800001)
        right = (0x3F800000, 0x40000000, 0xFF800000, 0xBF800000)
    elif mode == 3:
        left = (0x3F800000, 0x40000000, 0xFF800000, 0xBF800000)
        right = (0x7FC12345, 0x7F812345, 0x7FC54321, 0x7F800001)
    elif mode == 4:
        left = (0x7F800000, 0xFF800000, 0x3F800000, 0xBF800000)
        right = (0xFF800000, 0x7F800000, 0xBF800000, 0x3F800000)
    else:
        finite = (0x3E800000, 0xBF400000, 0x41200000, 0xC1200000,
                  0x00000000, 0x80000000, 0x7F800000, 0xFF800000)
        left = tuple(rng.choice(finite) for _ in range(4))
        right = tuple(rng.choice(finite) for _ in range(4))
    left_bytes = struct.pack("<4I", *left)
    right_bytes = struct.pack("<4I", *right)
    return [other], [(obj, left_bytes), (other, right_bytes)], obj


def _e37_sign_vector_direct_fuzz(_mu, heap, rng):
    obj = heap + 0x10000
    positive = (0x00000000, 0x80000000, 0x3F800000, 0x40000000, 0x7F800000)
    negative = (0x80000000, 0xBF800000, 0xC0000000, 0xFF800000)
    mode = rng.randrange(8)
    if mode == 0:
        lanes = tuple(rng.choice(positive) for _ in range(3))
    elif mode == 1:
        lanes = tuple(rng.choice(negative) for _ in range(3))
    elif mode in (2, 3):
        lanes = [rng.choice(positive if mode == 2 else negative) for _ in range(3)]
        lanes[rng.randrange(3)] = rng.choice(negative if mode == 2 else positive)
        lanes = tuple(lanes)
    elif mode == 4:
        lanes = (0x7FC12345, 0x7F812345, 0x3F800000)
    elif mode == 5:
        lanes = (0x3F800000, 0x7FC54321, 0x7F800001)
    else:
        finite = positive + negative
        lanes = tuple(rng.choice(finite) for _ in range(3))
    return [], [(obj, struct.pack("<3I", *lanes))], obj


def _e37_store_5630_direct_fuzz(_mu, heap, rng):
    obj, p2, p3, out = (heap + 0x10000, heap + 0x11000, heap + 0x12000, heap + 0x13000)
    finite = (0x00000000, 0x80000000, 0x3F800000, 0xBF800000, 0x40200000,
              0xC0500000, 0x41880000, 0xC1100000, 0x3DCCCCCD)
    values = [struct.pack("<3I", *(rng.choice(finite) for _ in range(3))) for _ in range(3)]
    alias = rng.randrange(4)
    if alias == 1:
        out = obj
    elif alias == 2:
        out = p2
    elif alias == 3:
        out = p3
    writes = [(obj, values[0]), (p2, values[1]), (p3, values[2])]
    if out not in (obj, p2, p3):
        writes.append((out, bytes(rng.getrandbits(8) for _ in range(12))))
    return [p2, p3, out], writes, obj


def _e37_dot_x_direct_fuzz(_mu, heap, rng):
    obj, b, c = heap + 0x10000, heap + 0x11000, heap + 0x12000
    finite = (0x00000000, 0x80000000, 0x3F800000, 0xBF800000, 0x40200000,
              0xC0500000, 0x41880000, 0xC1100000, 0x3DCCCCCD)
    writes = []
    for at in (obj, b, c):
        writes.append((at, struct.pack("<3I", *(rng.choice(finite) for _ in range(3)))))
    return [b, c], writes, obj


def _brake_wall_query_nodes(mu, args):
    wall = args[0]
    child = _read_u32(mu, wall + 0x78)
    if not child:
        return []
    nodes, pairs, seen = [child, child + 0x18], [(child, 0x30)], {child}
    while nodes:
        node = nodes.pop()
        branch = _read_u32(mu, node + 0x14)
        if branch and branch not in seen:
            seen.add(branch)
            pairs.append((branch, 0x30))
            nodes.extend((branch, branch + 0x18))
    return pairs


def _read_u32(mu, at):
    return struct.unpack("<I", bytes(mu.mem_read(at, 4)))[0]


def _cell_grid_cell(args, mu):
    grid, x_word, y_word = args
    f32 = lambda word: struct.unpack("<f", struct.pack("<I", word))[0]
    ox = struct.unpack("<f", bytes(mu.mem_read(grid, 4)))[0]
    oy = struct.unpack("<f", bytes(mu.mem_read(grid + 4, 4)))[0]
    rows = _read_u32(mu, grid + 8)
    columns = _read_u32(mu, grid + 12)
    cells = _read_u32(mu, grid + 16)
    ix = int(f32(x_word) - ox)
    iy = int(f32(y_word) - oy)
    ix = max(0, min(columns * 100 - 1, ix))
    iy = max(0, min(rows * 100 - 1, iy))
    cell = columns * (iy // 100) + ix // 100
    local = (ix % 100) // 10 + ((iy % 100) // 10) * 10
    return cells, cell, local


def _cell_grid_lookup_nodes(mu, args):
    cells, cell, _local = _cell_grid_cell(args, mu)
    count = _read_u32(mu, args[0] + 8) * _read_u32(mu, args[0] + 12)
    windows = [(cells, count * 4)]
    block = _read_u32(mu, cells + cell * 4)
    if block:
        windows.append((block - 4, 0x644))
    return windows


def _cell_grid_lookup_after(mu, args, _eax):
    cells, cell, _local = _cell_grid_cell(args, mu)
    block = _read_u32(mu, cells + cell * 4)
    return [(block - 4, 0x644)] if block else []


def _cell_grid_lookup_direct_fuzz(_mu, heap, rng):
    # Keep the fuzz objects above the stub allocator's bump region. The original operator new
    # uses heap_base for its first block and would otherwise overlap this grid during the call.
    grid, cells, block = heap + 0x10000, heap + 0x11000, heap + 0x12000
    case = rng.randrange(4)
    row, column = rng.randrange(3), rng.randrange(4)
    local_x, local_y = rng.choice((0, 9, 10, 29, 50, 99)), rng.choice((0, 9, 10, 39, 80, 99))
    x, y = column * 100 + local_x, row * 100 + local_y
    if case == 2:
        x, y = -5, -7
    elif case == 3:
        x, y = 800, 700
    cell_column = max(0, min(399, int(x))) // 100
    cell_row = max(0, min(299, int(y))) // 100
    cell = cell_row * 4 + cell_column
    existing = case in (1, 3)
    slots = [0] * 12
    slots[cell] = block + 4 if existing else 0
    writes = [
        (grid, struct.pack("<ffiiI", 0.0, 0.0, 3, 4, cells)),
        (cells, struct.pack("<12I", *slots)),
    ]
    if existing:
        writes.append((block, struct.pack("<I", 100) + bytes(0x640)))
    return [struct.unpack("<I", struct.pack("<f", x))[0],
            struct.unpack("<I", struct.pack("<f", y))[0]], writes, grid


def _e36_collect_nodes(mu, args):
    # g_7cbb00 points at the transform matrix used by E36T.
    matrix = _read_u32(mu, 0x7CBB00)
    body = args[1]
    vtable = _read_u32(mu, body)
    windows = [(0x7CBB00, 4), (vtable, 0x120)]
    if matrix:
        windows.append((matrix, 0x40))
    return windows


def _e36_nearby_owners(mu, object):
    objects = set()
    grid = _read_u32(mu, 0x8EB794)
    if grid:
        rows, columns = _read_u32(mu, grid + 8), _read_u32(mu, grid + 12)
        cells = _read_u32(mu, grid + 16)
        origin_x = struct.unpack("<f", bytes(mu.mem_read(grid, 4)))[0]
        origin_y = struct.unpack("<f", bytes(mu.mem_read(grid + 4, 4)))[0]
        center_x, center_y = struct.unpack("<2f", bytes(mu.mem_read(object + 0x10, 8)))
        for dx in (-15.0, -5.0, 5.0, 15.0):
            for dy in (-15.0, -5.0, 5.0, 15.0):
                ix, iy = int(center_x + dx - origin_x), int(center_y + dy - origin_y)
                ix = max(0, min(columns * 100 - 1, ix))
                iy = max(0, min(rows * 100 - 1, iy))
                cell = columns * (iy // 100) + ix // 100
                local = (ix % 100) // 10 + ((iy % 100) // 10) * 10
                block = _read_u32(mu, cells + cell * 4)
                if not block:
                    continue
                node = _read_u32(mu, block + local * 0x10)
                seen = set()
                while node and node not in seen and len(seen) < 10000:
                    seen.add(node)
                    owner = _read_u32(mu, node + 12)
                    if owner:
                        objects.add(owner)
                    node = _read_u32(mu, node)
    return objects


def _e36_collect_dispatch(mu, args):
    # Candidate owners near this box can use derived slot implementations.
    objects = _e36_nearby_owners(mu, args[0]) | {args[1]}
    targets = set()
    for obj in objects:
        vtable = _read_u32(mu, obj)
        if vtable:
            targets.add(_read_u32(mu, vtable + 70 * 4))
            targets.add(_read_u32(mu, vtable + 12 * 4))
    return [(target, 0) for target in sorted(targets) if target]


def _fuzz_e36_collect(mu, args, rng, put):
    # Force the near-grid candidates onto the OBB path, with large finite extents
    # so at least the closest candidates overlap the collector's box.
    object = args[0]
    for owner in sorted(_e36_nearby_owners(mu, object)):
        collision = _read_u32(mu, owner + 0x64)
        if not collision:
            continue
        shape = _read_u32(mu, collision + 0x680)
        if not shape:
            continue
        put(collision + 0x68A, 0, "<B")
        put(shape + 0x10, struct.unpack("<I", struct.pack("<f", 0.0))[0], "<I")
        put(shape + 0x14, struct.unpack("<I", struct.pack("<f", 0.0))[0], "<I")
        put(shape + 0x18, struct.unpack("<I", struct.pack("<f", 0.0))[0], "<I")
        for offset in (0x20, 0x24, 0x28):
            put(shape + offset, 10000.0)
        for offset in range(0x30, 0x60, 4):
            value = (1.0 if offset in (0x30, 0x44, 0x58) else 0.0)
            put(shape + offset, value)
        break


def _e36_lookup_nodes(mu, args):
    cells, cell, _local = _cell_grid_cell(args, mu)
    slot = cells + cell * 4
    block = _read_u32(mu, slot)
    return [(slot, 4)] + ([(block - 4, 0x644)] if block else [])


def _e36_lookup_after(mu, args, _eax):
    cells, cell, _local = _cell_grid_cell(args, mu)
    block = _read_u32(mu, cells + cell * 4)
    return [(block - 4, 0x644)] if block else []


def _e36_query_nodes(mu, args):
    physics = _read_u32(mu, 0x1659B70)
    cdef = _read_u32(mu, physics + 0x24)
    grid = _read_u32(mu, 0x8EB794)
    rows = _read_u32(mu, grid + 8)
    columns = _read_u32(mu, grid + 12)
    cells = _read_u32(mu, grid + 16)
    f32 = lambda value: struct.unpack("<f", struct.pack("<f", value))[0]
    a0, a1 = struct.unpack("<2f", bytes(mu.mem_read(args[0], 8)))
    b0, b1 = struct.unpack("<2f", bytes(mu.mem_read(args[1], 8)))
    s0 = f32(f32(a0 + b0) * 0.5)
    s1 = f32(f32(a1 + b1) * 0.5)
    flags = _read_u32(mu, cdef + 0x160CC)
    if flags & 0x2000:
        points = ((f32(s0 - 4.7), f32(s1 - 4.7)), (f32(s0 - 4.7), f32(s1 + 4.7)),
                  (f32(s0 + 4.7), f32(s1 - 4.7)), (f32(s0 + 4.7), f32(s1 + 4.7)))
    else:
        points = ((a0, a1), (a0, b1), (b0, a1), (b0, b1))
    windows = [(0x1659B70, 4), (physics + 0x24, 4), (cdef + 0x160CC, 4),
               (0x8EB794, 4), (grid, 0x14)]
    seen_windows = {(at, length) for at, length in windows}
    for x, y in points:
        ix, iy = int(f32(x - struct.unpack("<f", bytes(mu.mem_read(grid, 4)))[0])), int(
            f32(y - struct.unpack("<f", bytes(mu.mem_read(grid + 4, 4)))[0]))
        ix = max(0, min(columns * 100 - 1, ix))
        iy = max(0, min(rows * 100 - 1, iy))
        cell = columns * (iy // 100) + ix // 100
        local = (ix % 100) // 10 + ((iy % 100) // 10) * 10
        slot = cells + cell * 4
        block = _read_u32(mu, slot)
        addresses = [(slot, 4)]
        if block:
            link = block + local * 0x10
            addresses.append((link, 0x10))
            node = _read_u32(mu, link)
            seen_nodes = set()
            while node and node not in seen_nodes and len(seen_nodes) < 10000:
                seen_nodes.add(node)
                owner = _read_u32(mu, node + 12)
                addresses.append((node, 0x10))
                if owner:
                    addresses.append((owner, 0xD0))
                    vtable = _read_u32(mu, owner)
                    if vtable:
                        addresses.append((vtable, 0x120))
                    collision = _read_u32(mu, owner + 0x64)
                    if collision:
                        addresses.append((collision, 0x6B0))
                        shape = _read_u32(mu, collision + 0x680)
                        if shape:
                            addresses.append((shape, 0x300))
                node = _read_u32(mu, node)
        for at, length in addresses:
            if (at, length) not in seen_windows:
                seen_windows.add((at, length))
                windows.append((at, length))
    return windows


def _e36_compose_after(mu, args, _eax):
    if args[3] == args[1] + 0x10:
        length = 0x10
    else:
        collision = _read_u32(mu, args[0] + 0x64)
        try:
            flags = bytes(mu.mem_read(collision + 0x68A, 1))[0]
        except Exception:  # a call outside E36 (M_C1A0's callees, `this` the worker): no grid, no output window
            return []
        length = 0x60 if flags & 4 else 0xA0
    return [(args[3], length)]


def _cell_grid_ctor_direct_fuzz(_mu, heap, rng):
    grid = heap + 0x10000
    dimensions = ((0.0, 0.0), (0.5, 50.0), (99.75, 99.75), (100.0, 100.0),
                  (100.5, 199.99), (199.99, 100.5), (250.75, 350.25), (400.0, 300.0))
    width, height = rng.choice(dimensions)
    origin_x, origin_y = rng.uniform(-500.0, 500.0), rng.uniform(-500.0, 500.0)
    words = [struct.unpack("<I", struct.pack("<f", value))[0]
             for value in (origin_x, origin_y, width, height)]
    initial = struct.pack("<5I", *(rng.getrandbits(32) for _ in range(5)))
    return words, [(grid, initial)], grid


def _body_ctor_direct_fuzz(_mu, heap, rng):
    body = heap + 0x10000
    definition = heap + 0x18000 if rng.randrange(2) else 0
    name = 0x0073CBA0
    writes = [(body, struct.pack("<52I", *(rng.getrandbits(32) for _ in range(52))))]
    if definition:
        writes.append((definition, struct.pack("<3I", 0x72E444, name, rng.getrandbits(32))))
    definition_arg = definition
    frame_arg = heap + 0x17000 + 4 * rng.randrange(128)
    flags = rng.choice((0, 1, 2, 3, 0x20, 0x40, 0x80))
    return [definition_arg, frame_arg, 0, flags], writes, body


def _body_ctor_nodes(_mu, args):
    definition = args[1]
    return [(definition, 0x0C)] if definition else []


def _cell_grid_ctor_after(mu, args, _eax):
    cells = _read_u32(mu, args[0] + 0x10)
    if not cells:
        return []
    count = _read_u32(mu, args[0] + 8) * _read_u32(mu, args[0] + 12)
    return [(cells - 4, count * 4 + 4)]


def _notify_filtered_nodes(mu, args):
    nodes = []
    link = _read_u32(mu, args[0])
    while link:
        nodes.append((link, 0x10))
        body = _read_u32(mu, link + 0x0C)
        if body:
            nodes.append((body + 0x60, 4))
        link = _read_u32(mu, link)
    return nodes


def _notify_filtered_dispatch(mu, args):
    return [(vtarget(mu, args[1], 1), 4)]


def _notify_filtered_direct_fuzz(_mu, heap, rng):
    head = heap + 0x10000
    links, bodies = heap + 0x11000, heap + 0x12000
    visitor, vtable = heap + 0x13000, heap + 0x14000
    count = rng.randrange(0, 7)
    eligible = bool(rng.randrange(2))
    writes = [(head, struct.pack("<4I", links if count else 0, 0, 0, 0)),
              (visitor, struct.pack("<I", vtable)),
              (vtable, struct.pack("<II", 0, 0x4E3590))]
    for index in range(count):
        link = links + index * 0x10
        body = bodies + index * 0x70 if rng.randrange(4) != 0 or index == count - 1 else 0
        next_link = link + 0x10 if index + 1 < count else 0
        writes.append((link, struct.pack("<4I", next_link, 0, head, body)))
        if body:
            filtered = (index != count - 1) or not eligible
            flag = 0x20 if filtered else rng.choice((0, 0x04, 0x40))
            writes.append((body + 0x60, struct.pack("<I", flag)))
    return [visitor], writes, head


def _body_vfcc_nodes(_mu, _args):
    return [(0x007CBB00, 4)]


def _body_vfcc_fuzz(mu, args, rng, put):
    body, first, second = args
    worker = word(mu, 0x007CBB00)
    first_data, second_data = body + 0x140, body + 0x160
    for matrix, data in ((first, first_data), (second, second_data)):
        put(matrix, data, "<I")
        put(matrix + 4, worker, "<I")
    case = rng.randrange(3)
    first_values = [0.5, 0.5, 0.5, rng.uniform(-4.0, 4.0)]
    second_values = [0.5, 0.5, 0.5, rng.uniform(-4.0, 4.0)]
    if case == 1:
        first_values[rng.randrange(3)] = 2.0
    elif case == 2:
        second_values[rng.randrange(3)] = -1.0
    for index, value in enumerate(first_values):
        put(first_data + 4 * index, value)
    for index, value in enumerate(second_values):
        put(second_data + 4 * index, value)
    for index in range(3):
        put(body + 0x80 + 4 * index, 0.0)
        put(body + 0x90 + 4 * index, 1.0)


def _body_vfcc_branch(args, windows):
    body, first, second = windows[0][3], windows[3][3], windows[4][3]
    first_x = struct.unpack_from("<3f", first)
    second_x = struct.unpack_from("<3f", second)
    upper = struct.unpack_from("<3f", body, 0x90)
    lower = struct.unpack_from("<3f", body, 0x80)
    hit = all(first_x[i] - upper[i] < 0.0 and lower[i] - second_x[i] < 0.0 for i in range(3))
    return "hit" if hit else "miss"


def _select_table_nodes(mu, args):
    obj = args[0]
    if word(mu, obj + 0x44) & 0x30 in (0x10, 0x30):
        pointer = word(mu, obj + 0x34)
        nested = word(mu, pointer)
        return [(pointer, 4), (nested, 4)]
    return []


def _select_table_branch(args, windows):
    mode = struct.unpack_from("<I", windows[0][3], 0x44)[0] & 0x30
    return {0: "indexed", 0x10: "indirect", 0x20: "pointer", 0x30: "indirect"}[mode]


def _fuzz_select_table(mu, args, rng, put):
    obj = args[1]
    mode = rng.choice((0, 0x10, 0x20, 0x30))
    put(obj + 0x44, (word(mu, obj + 0x44) & ~0x30) | mode, "<I")


def _fuzz_body_clear_grid(_mu, args, rng, put):
    if rng.randrange(2) == 0:
        put(args[0] + 0x64, 0, "<I")


def _make_event_after(_mu, _args, eax):
    return [(eax, 0xF0)] if eax else []


def _body_def_ctor_direct_args(_rng, _this):
    return [0x737C24]


def _body_state_ctor_direct_args(_rng, state):
    body = state - 0x70
    return [body, 0x737C24, body]


def _fuzz_body_def_ctor(_mu, args, rng, put):
    this = args[0]
    for offset in range(0, 0x20, 4):
        put(this + offset, rng.getrandbits(32), "<I")


def _fuzz_body_def_dtor(_mu, args, rng, put):
    _fuzz_body_def_ctor(_mu, args, rng, put)
    put(args[0], 0x737BC4, "<I")
    this = args[0]
    case = rng.randrange(5)
    if case == 0:
        put(this + 0x14, 0, "<I")
        return

    owner, next_node, prev_node = this + 0x40, this + 0x50, this + 0x70
    next_ptr = next_node if case in (2, 4) else 0
    prev_ptr = prev_node if case in (3, 4) else 0
    put(this + 0x0C, next_ptr, "<I")
    put(this + 0x10, prev_ptr, "<I")
    put(this + 0x14, owner, "<I")
    put(owner, this + 0x0C, "<I")
    put(owner + 4, this + 0x0C, "<I")
    put(owner + 8, rng.randrange(1, 10), "<I")
    for node in (next_node, prev_node):
        put(node + 0x0C, rng.getrandbits(32), "<I")
        put(node + 0x10, rng.getrandbits(32), "<I")


def _body_def_dtor_nodes(mu, args):
    this = args[0]
    read = lambda at: struct.unpack("<I", bytes(mu.mem_read(at, 4)))[0]
    owner, next_ptr, prev_ptr = read(this + 0x14), read(this + 0x0C), read(this + 0x10)
    if owner == 0:
        return []
    out = [(owner, 0x0C)]
    out.extend((ptr, 0x14) for ptr in (next_ptr, prev_ptr) if ptr)
    return out


def _fuzz_body_state_ctor(_mu, args, rng, put):
    state, body = args[:2]
    for offset in (0x00, 0x04, 0x08, 0x30, 0x34, 0x38, 0x3C, 0x40):
        put(state + offset, rng.getrandbits(32), "<I")
    put(body + 0x60, rng.choice((0, 1, 2, 3, 0xFFFFFFFF)), "<I")


def _fuzz_state_unka44(_mu, args, rng, put):
    state = args[0]
    pool = (-1000.0, -1.0, -0.0, 0.0, 0.25, 1.0, 1000.0)
    for offset in range(0x10, 0x30, 4):
        put(state + offset, rng.choice(pool + (rng.uniform(-1000.0, 1000.0),)))


def _fuzz_scratch_ctor(_mu, args, rng, put):
    put(args[0], rng.getrandbits(32), "<I")


def _ray_object_direct_fuzz(_mu, heap, rng):
    # The 0x6b0-byte ray object (E38RaySeeds and E38RayClear clear its fields and leave the rest).
    obj = heap + 0x10000
    return [], [(obj, bytes(rng.randrange(256) for _ in range(0x6B0)))], obj


def _grid_add_edge_direct_fuzz(_mu, heap, rng):
    # E37GridOps::AddEdge: a 0x6b0-byte grid whose count byte (+0x68d) is 0 to 40 (0 is the early return), random
    # points, and the vector added to their three first lanes.
    grid, vec = heap + 0x10000, heap + 0x11000
    block = bytearray(rng.randrange(256) for _ in range(0x6B0))
    for k in range(4 * 41):
        struct.pack_into("<f", block, 0x40 + 4 * k, rng.uniform(-500.0, 500.0))
    block[0x68D] = rng.choice([0, 1, 2, rng.randrange(3, 41)])
    return [vec], [(grid, bytes(block)), (vec, struct.pack("<4f", *(rng.uniform(-50.0, 50.0) for _ in range(4))))], grid


def _m_d760x_nodes(mu, a):
    """M_D760X: the list (the grid's own when the argument is null), its records, their faces and vertex blocks."""
    grid = a[0]
    list_at = a[4] or grid + 0x1014
    count = min(_read_u32(mu, list_at), 16)
    windows = [(list_at, 4 + 4 * count)]
    for index in range(count):
        record = _read_u32(mu, list_at + 4 + 4 * index)
        windows.append((record, 0x20))
        word = _read_u32(mu, record + 0x18)
        faces = word & 0x1FFFFF
        if faces:
            windows.append((_read_u32(mu, record + 0x1C), 0x12 * faces))
        base = _read_u32(mu, grid + 0xC + 4 * (word >> 22))
        if base:
            windows.append((base, 12 * 6))
    return windows


def _m_d760x_direct_fuzz(_mu, heap, rng):
    # A grid with one vertex block (table entry 1, at +0x10), and up to three records near the ray. Some faces are hit, some
    # are not; the list is the grid's own (+0x1014) or an explicit one.
    grid, base = heap + 0x10000, heap + 0x20000
    start, end, out = heap + 0x50000, heap + 0x50010, heap + 0x50100
    start_xy = [rng.uniform(-1.0, 1.0) for _ in range(2)]
    start_z = rng.choice((-1.0, 1.0))
    end_z = -start_z * rng.choice((0.5, 1.0, 1.5))
    writes = [
        (grid, bytes(0x1040)),
        (start, struct.pack("<3f", *start_xy, start_z)),
        (end, struct.pack("<3f", *start_xy, end_z)),
        (out, bytes(0x40)),
    ]
    vertices = 6
    # The vertices surround the ray's xy (within a unit), so the faces cover it on about half the calls.
    verts = [(start_xy[0] + rng.uniform(-1.0, 1.0), start_xy[1] + rng.uniform(-1.0, 1.0), rng.uniform(-0.2, 0.2))
             for _ in range(vertices)]
    writes.append((base, b"".join(struct.pack("<3f", *v) for v in verts)))
    writes.append((grid + 0x10, struct.pack("<I", base)))
    records = []
    for index in range(rng.randrange(1, 4)):
        record, faces = heap + 0x30000 + 0x40 * index, heap + 0x40000 + 0x100 * index
        count = rng.choice((0, 1, 2, 3, 3))  # no faces: the record is skipped
        center = [start_xy[0] + rng.uniform(-0.5, 0.5), start_xy[1] + rng.uniform(-0.5, 0.5), rng.uniform(-0.5, 0.5)]
        half = [rng.uniform(0.5, 2.0) for _ in range(3)]
        writes.append((record, struct.pack("<6f", *center, *half)
                       + struct.pack("<II", (1 << 22) | count, faces)))
        face_bytes = b""
        for _ in range(count):
            corners = [rng.randrange(vertices) for _ in range(3)]
            # The ray's determinant is positive when the normal's z agrees with the ray's: the triangle must face it.
            (x0, y0, _), (x1, y1, _), (x2, y2, _) = (verts[c] for c in corners)
            if (start_z < 0) != ((x1 - x0) * (y2 - y0) - (y1 - y0) * (x2 - x0) > 0):
                corners[1], corners[2] = corners[2], corners[1]
            face_bytes += struct.pack("<3H2H", *corners, rng.randrange(0x10000), rng.randrange(0x10000))
            face_bytes += bytes(rng.randrange(256) for _ in range(8))
        writes.append((faces, face_bytes))
        records.append(record)
    explicit = rng.random() < 0.5
    list_at = heap + 0x28000 if explicit else grid + 0x1014
    writes.append((list_at, struct.pack("<I", len(records)) + b"".join(struct.pack("<I", r) for r in records)))
    return [start, end, out, list_at if explicit else 0], writes, grid


def _node_walk_nodes(mu, a):
    """E38NodeWalk: the 8-node arena at the root (the fuzz lays the tree out there)."""
    return [(a[1], 0x40 * 8)] if a[1] else []


def _node_walk_direct_fuzz(_mu, heap, rng):
    # A chain of up to seven 0x40-byte nodes: box at +0, faces word at +0x18, child at +0x1c, second box at +0x20 (the next
    # child, whose faces word is at its +0x18, i.e. this node's +0x38).
    query, arena, out = heap + 0x50000, heap + 0x60000, heap + 0x50100
    count = rng.randrange(1, 8)

    def box():
        return [rng.uniform(-1.5, 1.5) for _ in range(3)] + [rng.uniform(0.25, 1.5) for _ in range(3)]

    writes = [(query, struct.pack("<6f", *box())), (out, bytes(0x100))]
    for index in range(count):
        node = bytearray(0x40)
        struct.pack_into("<6f", node, 0, *box())
        struct.pack_into("<I", node, 0x18, rng.choice((0, 0, 1, 2)) | (0x200000 if rng.random() < 0.3 else 0))
        child = arena + 0x40 * (index + 1) if index + 1 < count and rng.random() < 0.7 else 0
        struct.pack_into("<I", node, 0x1C, child)
        struct.pack_into("<6f", node, 0x20, *box())
        struct.pack_into("<I", node, 0x38, rng.choice((0, 0, 1)) | (0x200000 if rng.random() < 0.3 else 0))
        writes.append((arena + 0x40 * index, bytes(node)))
    root = arena if rng.random() < 0.9 else 0
    return [query, root, out], writes, None



# The E38 solver passes (0x4fb7f0, 0x4fbc20, 0x4fc0b0, 0x4fc590, 0x4fca20) on a solver the recipe builds in the stub
# heap: up to four events on two bodies. The bodies are CSolid-shaped (hook +0x3cc and ring +0x3c8 null, the inverse mass
# at +0x16c of a copy of its def at +0x34, as rigid_body.py's fuzz_vf128; copies of the snapshot's CSolid, so the
# vcu-cov snapshots only) with a copy of CSolid's vtable (0x738b44) whose slot 0x90
# (the heavy wake, 0x52bf30) is the empty 0x4fb820; the events have CCollisionEvent's vtable (0x739558).
E38_SOLVER = 0x100000  # heap offsets
E38_VTABLE, E38_BODIES, E38_DEFS, E38_EVENTS = 0x1000, (0x2000, 0x2400), (0x3000, 0x3200), 0x4000
E38_CSOLID_VTABLE = 0x738B44
E38_EVENT_VTABLE = 0x739558
E38_SNAPSHOT_SOLID = 0xBF1D7B8  # a CSolid in the vcu-cov snapshots (rigid_body.py's fuzz_vf128 reads its vtable)


def _word(mu, at):
    return struct.unpack("<I", bytes(mu.mem_read(at, 4)))[0]


def _e38_solver_direct_fuzz(mu, heap, rng):
    solver = heap + E38_SOLVER
    vtable = bytearray(mu.mem_read(E38_CSOLID_VTABLE, 0x200))
    struct.pack_into("<I", vtable, 0x90, 0x4FB820)
    writes = [(heap + E38_VTABLE, bytes(vtable)), (solver, bytes(0xC430 + 0x90 * 4))]
    bodies = [heap + b for b in E38_BODIES]
    active = {}
    for body, definition in zip(bodies, (heap + d for d in E38_DEFS)):
        # A copy of the snapshot's CSolid (its frame and entity pointers stay valid for M_C1A0's callees).
        data = bytearray(mu.mem_read(E38_SNAPSHOT_SOLID, 0x3D0))
        struct.pack_into("<I", data, 0, heap + E38_VTABLE)
        struct.pack_into("<I", data, 0x34, definition)
        for at in (0x274, 0x3B0, 0x3C8, 0x3CC):
            struct.pack_into("<I", data, at, 0)
        data[0x60] = 4 if rng.random() < 0.8 else 0
        active[body] = data[0x60] != 0
        struct.pack_into("<II", data, 0x1DC, rng.choice((0, 0, 1)), 0)
        struct.pack_into("<I", data, 0x1EC, rng.choice((0, 0, 1)))
        struct.pack_into("<f", data, 0x290, rng.uniform(0.0, 1.0))
        for k in range(3):
            struct.pack_into("<f", data, 0x140 + 4 * k, rng.uniform(-10.0, 10.0))
            struct.pack_into("<f", data, 0x190 + 4 * k, rng.uniform(-3.0, 3.0))
            struct.pack_into("<f", data, 0x1C0 + 4 * k, rng.uniform(-10.0, 10.0))
        writes.append((body, bytes(data)))
        block = bytearray(mu.mem_read(_word(mu, E38_SNAPSHOT_SOLID + 0x34), 0x1F0))
        struct.pack_into("<f", block, 0x16C, rng.choice((0.0, rng.uniform(0.001, 0.1))))
        for k in range(3):
            struct.pack_into("<f", block, 0x130 + 4 * k, rng.uniform(0.0, 0.05))
        writes.append((definition, bytes(block)))
    count = rng.randrange(1, 5)
    events = [heap + E38_EVENTS + 0x100 * k for k in range(count)]
    for k, event in enumerate(events):
        data = bytearray(0x100)
        struct.pack_into("<I", data, 0, E38_EVENT_VTABLE)
        struct.pack_into("<III", data, 0xC, events[k + 1] if k + 1 < count else 0, events[k - 1] if k else 0,
                         solver + 4)
        normal = [rng.uniform(-1.0, 1.0) for _ in range(3)]
        length = sum(v * v for v in normal) ** 0.5 or 1.0
        struct.pack_into("<4f", data, 0x70, *[v / length for v in normal], rng.uniform(-1.0, 1.0))
        for at in (0x60, 0xC0):
            struct.pack_into("<3f", data, at, *[rng.uniform(-1.0, 1.0) for _ in range(3)])
        for at in (0x80, 0xAC):
            struct.pack_into("<IIf", data, at, rng.randrange(4), rng.randrange(4), rng.uniform(0.0, 1.0))
        struct.pack_into("<II", data, 0x8C, rng.randrange(4), rng.randrange(4))
        struct.pack_into("<II", data, 0xA4, rng.randrange(4), rng.randrange(4))
        struct.pack_into("<f", data, 0x94, rng.uniform(0.0, 1.2))
        body, partner = bodies if rng.random() < 0.8 else bodies[::-1]
        struct.pack_into("<I", data, 0x98, partner)
        struct.pack_into("<I", data, 0xA0, body)
        flags = rng.choice((0, 0x800, 0x1000, 0x1800, 0x1800, 0x800, 0x1000))
        if not active[partner]:
            flags &= ~0x1000  # as the game sets it (0x4fbc20); the pass would call the partner's rows on null
        if rng.random() < 0.15:
            flags |= rng.choice((0x2000, 0x4000))
        struct.pack_into("<I", data, 0xE4, flags)
        writes.append((event, bytes(data)))
    rows = rng.randrange(3)
    header = struct.pack("<III", events[0], events[-1], count)
    writes.append((solver + 4, header))
    writes.append((solver + 0x18, struct.pack("<I", rows)))
    for k in range(rows):
        writes.append((solver + 0x20 + 0x310 * k, struct.pack("<I", rng.choice((bodies[k], 0)))))
    return [], writes, solver


def _e38_solver_nodes(mu, args):
    solver = args[0]
    heap = solver - E38_SOLVER
    manager = _word(mu, _word(mu, 0x01659B70) + 0x24) + 0x18
    nodes = [(solver, 0x20 + 0x310 * 2), (solver + 0xC420, 0x10 + 0x90 * 2), (heap + E38_VTABLE, 0x200),
             (heap + E38_EVENTS, 0x400), (0x01659B70, 4), (_word(mu, 0x01659B70) + 0x24, 4),
             (manager, 0x160 * 4 + 0x10), (0x8EF7F0, 0x10),
             (E38_EVENT_VTABLE + 0x1C, 4)]
    nodes += [(heap + b, 0x3D0) for b in E38_BODIES]
    free_head = _word(mu, 0x8EF7F4)
    if free_head:
        nodes.append((free_head + 0x10, 4))
    return nodes


def _e38_solver_events(windows):
    """(flags, body active, partner active, time, body progress, partner progress) of each event in the windows."""
    by_at = {at: data for _, at, _, data in windows}
    solver = windows[0][1]
    heap = solver - E38_SOLVER
    events, bodies = by_at[heap + E38_EVENTS], {heap + b: by_at[heap + b] for b in E38_BODIES}
    out = []
    for k in range(struct.unpack_from("<I", by_at[solver], 0xC)[0]):
        body, partner = (bodies[struct.unpack_from("<I", events, 0x100 * k + at)[0]] for at in (0xA0, 0x98))
        out.append((struct.unpack_from("<I", events, 0x100 * k + 0xE4)[0], body[0x60] & 4 != 0, partner[0x60] & 4 != 0,
                    struct.unpack_from("<f", events, 0x100 * k + 0x94)[0], struct.unpack_from("<f", body, 0x290)[0],
                    struct.unpack_from("<f", partner, 0x290)[0]))
    return out


def _e38_correction_branch(_args, windows):
    """E38_004FC590X: the moves its events take (the active bodies' flag bits 11 and 12, suppressed or not)."""
    kinds = set()
    for flags, body, partner, *_ in _e38_solver_events(windows):
        if body and flags & 0x1800:
            kinds.add({0x800: "body", 0x1000: "partner", 0x1800: "split"}[flags & 0x1800] +
                      ("-supp" if flags & 0x6000 else "") + ("" if partner else "-alone"))
    return "+".join(sorted(kinds)) or "none"


def _e38_due_branch(_args, windows):
    """E38_004FCA20: how many of its events are due (time - 0.1 not above the smaller progress)."""
    due = sum(1 for _, _, partner, time, p, q in _e38_solver_events(windows)
              if not (time - 0.1 > (min(p, q) if partner else p)))
    return "due%d" % due


def _e38_solver_spec(name, dispatch, branch=None):
    spec = dict(name=name, popped=0, x87=False, thiscall=True, direct=True, windows=[], nodes=_e38_solver_nodes,
                dispatch=lambda _mu, _a: dispatch, direct_fuzz=_e38_solver_direct_fuzz)
    if branch:
        spec["branch"] = branch
    return spec


def _e38_event_bodies(mu, args, _eax):
    return [(_word(mu, args[1] + 0xA0), 0x3D0), (_word(mu, args[1] + 0x98), 0x3D0)]


def _e38_row_body(mu, args, _eax):
    body = _word(mu, args[0])
    return [(body + 0x140, 0x10), (body + 0x3B0, 4)]


def _e38_impulse_direct_fuzz(mu, heap, rng):
    """E38Ops::M_C1A0 (direct) on the first event of an _e38_solver_direct_fuzz solver, its flags drawn again so that
    most calls are solved (both sides movable) and some are suppressed (0x2000/0x4000) or one-sided."""
    _words, writes, solver = _e38_solver_direct_fuzz(mu, heap, rng)
    event = heap + E38_EVENTS
    flags = rng.choice((0x1800, 0x1800, 0x1800, 0x800, 0x1000, 0))
    if rng.random() < 0.1:
        flags |= rng.choice((0x2000, 0x4000))
    writes.append((event + 0xE4, struct.pack("<I", flags)))
    return [event], writes, solver


def _e38_impulse_nodes(mu, args):
    """What M_C1A0 reads besides its event: the material manager's first tags (E38_004FB720) and its two pointers."""
    manager = _word(mu, _word(mu, 0x01659B70) + 0x24) + 0x18
    return [(0x01659B70, 4), (_word(mu, 0x01659B70) + 0x24, 4), (manager, 0x160 * 4 + 0x10)]


def _e38_event_bodies_of_this(mu, args, _eax):
    return [(_word(mu, args[0] + 0xA0), 0x3D0), (_word(mu, args[0] + 0x98), 0x3D0)]


def _edge_edge_nodes(mu, args):
    grid = args[0]
    nodes = []
    for offset, size in ((0x400, 0x200), (0x408, 0x200), (0x40C, 0x200)):
        at = _word(mu, grid + offset)
        if at:
            nodes.append((at, size))
    return nodes


def _edge_edge_direct_fuzz(_mu, heap, rng):
    """Oblique hits, axis/cross misses, invalid material, degenerates, samples, and aliased outs for EdgeEdge."""
    grid = heap + 0x10000
    start_at, end_at = heap + 0x12000, heap + 0x12010
    point_at, distance_at, normal_at, word_at = (heap + 0x12020, heap + 0x12030,
                                                  heap + 0x12040, heap + 0x12050)
    row_at = grid + 0x600
    data_at, table_a, table_b = heap + 0x14000, heap + 0x14200, heap + 0x14400
    case = rng.randrange(8)
    hit_case = case < 6
    bary_u, bary_v = rng.uniform(0.15, 0.35), rng.uniform(0.15, 0.35)
    if bary_u + bary_v > 0.75:
        bary_v = 0.75 - bary_u
    base = tuple(rng.uniform(-0.8, 0.8) for _ in range(3))
    edge1 = (rng.uniform(1.0, 2.0), rng.uniform(-0.4, 0.4), rng.uniform(-0.4, 0.4))
    edge2 = (rng.uniform(-0.4, 0.4), rng.uniform(1.0, 2.0), rng.uniform(-0.4, 0.4))
    vertices = [base, tuple(base[k] + edge1[k] for k in range(3)),
                tuple(base[k] + edge2[k] for k in range(3))]
    hit = tuple(base[k] + bary_u * edge1[k] + bary_v * edge2[k] for k in range(3))
    cross = (edge1[1] * edge2[2] - edge1[2] * edge2[1],
             edge1[2] * edge2[0] - edge1[0] * edge2[2],
             edge1[0] * edge2[1] - edge1[1] * edge2[0])
    cross_len = math.sqrt(sum(value * value for value in cross))
    normal = tuple(value / cross_len for value in cross)
    dz = rng.uniform(0.3, 0.7)
    half = tuple(value * dz for value in normal)
    if case == 1:
        # Stay on the triangle path but put its segment outside the box on an axis.
        centre = (hit[0] + 3.0, hit[1], hit[2])
    elif case == 2:
        # Each component passes the axis checks, while the segment/box cross axis separates them.
        half = (1.0, 1.0, 1.0)
        centre = (hit[0] - 1.05, hit[1] + 1.05, hit[2])
    elif case == 6:
        # A zero-length segment whose midpoint is centered in the grid reaches the degenerate-edge path.
        centre = tuple(hit[k] + half[k] for k in range(3))
    else:
        centre = (hit[0] + rng.uniform(-0.2, 0.2), hit[1] + rng.uniform(-0.2, 0.2),
                  hit[2] + rng.uniform(-0.2, 0.2))
    start = tuple(hit[k] + half[k] for k in range(3)) + (rng.uniform(-2.0, 2.0),)
    end = tuple(hit[k] - half[k] for k in range(3)) + (rng.uniform(-2.0, 2.0),)
    if case == 6:
        end = start

    grid_bytes = bytearray(0x6B0)
    struct.pack_into("<3f", grid_bytes, 0x20, *centre)
    struct.pack_into("<3f", grid_bytes, 0x30, 0.3, 0.3, 0.3)
    struct.pack_into("<I", grid_bytes, 0x410, row_at)
    # EdgeEdge does not read +0x680; keep the fuzz case for the record's branch label.
    grid_bytes[0x680] = case
    face_count = 2 if case == 5 else 1
    grid_bytes[0x68B] = face_count
    grid_bytes[0x68F] = rng.randrange(256)
    for face in range(face_count):
        material = 0xff if case == 5 and face == 0 else rng.randrange(0xff)
        row = 0x600 + face * 7
        grid_bytes[row:row + 7] = bytes((0, 1, 2, material, 0, 1, 2))
    if case == 7:
        vertices[2] = vertices[1]
    for index, vertex in enumerate(vertices):
        struct.pack_into("<3f", grid_bytes, 0x40 + index * 0x10, *vertex)

    writes = [(grid, bytes(grid_bytes)),
              (start_at, struct.pack("<4f", *start)),
              (end_at, struct.pack("<4f", *end)),
              (point_at, bytes([0xCC]) * 16), (distance_at, bytes([0xCC]) * 4),
              (normal_at, bytes([0xCC]) * 16), (word_at, bytes([0xCC]) * 4)]
    if case % 2 == 0:
        struct.pack_into("<IHHII", grid_bytes, 0x400, data_at, 16, 16, table_a, table_b)
        writes[0] = (grid, bytes(grid_bytes))
        sample_data = bytearray(0x200)
        sample_data[0:6] = bytes((64, 64, 128, 64, 64, 128))
        writes += [(data_at, bytes(sample_data)),
                   (table_a, bytes((index * 7 + 13) & 0xFF for index in range(0x200))),
                   (table_b, bytes((251 - index * 5) & 0xFF for index in range(0x200)))]

    alias = rng.randrange(6)
    if alias == 1:
        word_at = distance_at
    elif alias == 2:
        point_at = start_at
    elif alias == 3:
        normal_at = start_at
    elif alias == 4:
        point_at = normal_at
    elif alias == 5:
        distance_at = normal_at

    return [start_at, end_at, point_at, distance_at, normal_at, word_at], writes, grid


def _edge_edge_branch(_args, windows):
    cases = ("sampled_oblique_hit", "overlap_axis_miss", "overlap_cross_miss", "oblique_hit",
             "sampled_oblique_hit", "material_skip_then_hit", "degenerate_segment", "degenerate_face")
    return cases[windows[0][3][0x680]]


SPECS = {
    0x4E0080: dict(name="CBodyLink::NotifyFiltered", popped=4, x87=False, thiscall=True, direct=True,
                   windows=[(0, lambda a: 0x10), (1, lambda a: 4), (1, lambda a: 8, 0)],
                   nodes=_notify_filtered_nodes, direct_fuzz=_notify_filtered_direct_fuzz,
                   dispatch=_notify_filtered_dispatch),
    0x4F8280: dict(name="E37GridOps::EdgeEdge", popped=0x18, x87=False, thiscall=True, external=True,
                   windows=[(0, lambda a: 0x6B0), (1, lambda a: 0x10), (2, lambda a: 0x10),
                            (3, lambda a: 0x10), (4, lambda a: 4), (5, lambda a: 0x10), (6, lambda a: 4)],
                   nodes=_edge_edge_nodes, branch=_edge_edge_branch, direct_fuzz=_edge_edge_direct_fuzz),
    0x4E0550: dict(name="CCellGrid::CCellGrid", popped=16, x87=False, thiscall=True, direct=True,
                   windows=[(0, lambda a: 0x14)], direct_fuzz=_cell_grid_ctor_direct_fuzz,
                   after=_cell_grid_ctor_after),
    0x4E3460: dict(name="CBody::CBody", popped=16, x87=False, thiscall=True, direct=True,
                   windows=[(0, lambda a: 0xd0)], nodes=_body_ctor_nodes,
                   direct_fuzz=_body_ctor_direct_fuzz),
    0x51A640: dict(name="CEntity::CEntity", popped=16, x87=False, thiscall=True, external=True,
                   windows=[(0, lambda a: 0x64)]),
    0x4E03B0: dict(name="CCellGrid::LookupTwin", popped=8, x87=False, thiscall=True, direct=True,
                   windows=[(0, lambda a: 0x14)], nodes=_cell_grid_lookup_nodes,
                   direct_fuzz=_cell_grid_lookup_direct_fuzz, after=_cell_grid_lookup_after),
    0x4DFFB0: dict(name="CCellRef::CCellRef", popped=0, x87=False, thiscall=True, external=True,
                   windows=[(0, lambda a: 4)]),
    0x4E3590: dict(name="CBody::CScratch::Vf00", popped=4, x87=False, thiscall=True, external=True,
                   windows=[(0, lambda a: 4)]),
    0x4E0000: dict(name="CBodyLink::Head", popped=0, x87=False, thiscall=True,
                   windows=[(0, lambda a: 0x10)]),
    0x4E0010: dict(name="CBodyLink::CBodyLink", popped=0, x87=False, thiscall=True,
                   windows=[(0, lambda a: 0x10)], direct=True),
    0x4E0020: dict(name="CBodyLink::Unlink", popped=0, x87=False, thiscall=True,
                   windows=[(0, lambda a: 0x10)], nodes=_link_nodes, fuzz=_fuzz_unlink, direct=True),
    0x4E01D0: dict(name="CCellLink::CCellLink", popped=0, x87=False, thiscall=True,
                   windows=[(0, lambda a: 0x10)], direct=True),
    0x4E0BD0: dict(name="E33Dict::Unk0C", popped=0, x87=False, thiscall=True, direct=True,
                   windows=[(0, lambda a: 0x14), (0, lambda a: 0x14, 0x10),
                            (0, lambda a: 0x24, 0x10, 0)],
                   fuzz=_fuzz_dict, dispatch=_dict_count_dispatch),
    0x4E0B70: dict(name="E33Dict::Unk18", popped=8, x87=False, thiscall=True, direct=True,
                   windows=[(0, lambda a: 0x14), (0, lambda a: 0x24, 0)],
                   direct_args=_dict_lookup_direct_args, fuzz=_fuzz_dict,
                   dispatch=lambda mu, a: [(vtarget(mu, a[0], 0), 0)]),
    0x4E0C00: dict(name="E33Dict::Unk10", popped=4, x87=False, thiscall=True, direct=True,
                   windows=[(0, lambda a: 0x14), (0, lambda a: 0x14, 0x10),
                            (0, lambda a: 0x24, 0x10, 0)],
                   direct_args=_dict_entry_direct_args, fuzz=_fuzz_dict,
                   dispatch=_dict_entry_dispatch),
    0x4E0D00: dict(name="E33MakeEvent", popped=36, x87=False, cdecl=True, direct=True,
                   windows=[(1, lambda a: 0x10), (3, lambda a: 0x68), (4, lambda a: 0x68),
                            (4, lambda a: 0x6B0, 0x64), (3, lambda a: 0x6B0, 0x64),
                            (7, lambda a: 0x10)],
                   nodes=_make_event_nodes, after=_make_event_after,
                   dispatch=_make_event_dispatch),
    0x4E0FD0: dict(name="CBody::CDef::CDef", popped=4, x87=False, thiscall=True, direct=True,
                   windows=[(0, lambda a: 0x20)], direct_args=_body_def_ctor_direct_args,
                   fuzz=_fuzz_body_def_ctor),
    0x4E1000: dict(name="CBody::CDef::~CDef", popped=0, x87=False, thiscall=True,
                   direct=True, windows=[(0, lambda a: 0x20)],
                   nodes=_body_def_dtor_nodes, fuzz=_fuzz_body_def_dtor),
    0x4E0FF0: dict(name="CBody::CDef::Vf4c", popped=0, x87=False, thiscall=True,
                   windows=[(0, lambda a: 0x20)]),
    0x4E1050: dict(name="CBody::CState::Vf20", popped=0, x87=False, thiscall=True,
                   windows=[(0, lambda a: 0x44)]),
    0x4E1060: dict(name="CBody::CState::Vf1C", popped=0, x87=False, thiscall=True,
                   windows=[(0, lambda a: 0x44)]),
    0x4E1070: dict(name="CBody::CState::CState", popped=12, x87=False, thiscall=True, direct=True,
                   windows=[(0, lambda a: 0x44), (1, lambda a: 0x68)],
                   direct_args=_body_state_ctor_direct_args, fuzz=_fuzz_body_state_ctor),
    0x4E1120: dict(name="CBody::Vf40", popped=0, x87=False, thiscall=True, direct=True,
                   windows=[(0, lambda a: 0xD0)]),
    0x4E1230: dict(name="CBody::VfCC", popped=8, x87=False, thiscall=True, direct=True,
                   windows=[(0, lambda a: 0xD0), (1, lambda a: 8), (2, lambda a: 8),
                            (1, lambda a: 0x10, 0), (2, lambda a: 0x10, 0)],
                   nodes=_body_vfcc_nodes, direct_args=_body_vfcc_direct_args,
                   fuzz=_body_vfcc_fuzz, branch=_body_vfcc_branch),
    0x4E1140: dict(name="CBody::Vf44", popped=4, x87=False, thiscall=True, direct=True,
                   windows=[(0, lambda a: 0xD0)], direct_args=_body_vf44_direct_args),
    0x4E11A0: dict(name="CBody::ClearGrid", popped=12, x87=False, thiscall=True, direct=True,
                   windows=[(0, lambda a: 0xD0)], direct_args=_body_clear_grid_direct_args,
                   fuzz=_fuzz_body_clear_grid),
    0x4E10E0: dict(name="CBody::CState::Uninit", popped=0, x87=False, thiscall=True,
                   windows=[(0, lambda a: 0x44)]),
    0x4E1100: dict(name="CBody::CState::GetOwner", popped=0, x87=False, thiscall=True,
                   windows=[(0, lambda a: 0x44)]),
    0x4E1110: dict(name="CBody::CScratch::Vf04", popped=0, x87=False, thiscall=True,
                   windows=[(0, lambda a: 4)]),
    0x4E33E0: dict(name="CBody::CState::Unk44", popped=0, x87=False, thiscall=True, direct=True,
                   windows=[(0, lambda a: 0x44), (0, lambda a: 0xD0, 0x30)],
                   dispatch=_state_unka44_dispatch, fuzz=_fuzz_state_unka44),
    0x4E3440: dict(name="CBody::CScratch::CScratch", popped=0, x87=False, thiscall=True,
                   windows=[(0, lambda a: 4)], fuzz=_fuzz_scratch_ctor),
    0x4E38E0: dict(name="CBrakeWall::Vf88", popped=0, x87=False, thiscall=True, direct=True,
                   windows=[(0, lambda a: 0x21C)], direct_fuzz=_brake_wall_direct_fuzz),
    0x4E3860: dict(name="CBrakeWall::Vf110", popped=0, x87=False, thiscall=True, direct=True,
                   windows=[(0, lambda a: 0x21C)], direct_fuzz=_brake_wall_direct_fuzz),
    0x4E3870: dict(name="CBrakeWall::Vf11C", popped=8, x87=False, thiscall=True, direct=True,
                   windows=[(0, lambda a: 0x21C), (1, lambda a: 0x10), (2, lambda a: 0x10)],
                   direct_fuzz=_brake_wall_direct_fuzz),
    0x4E3DC0: dict(name="CBrakeWall::QueryPoint", popped=8, x87=False, thiscall=True, direct=True,
                   windows=[(0, lambda a: 0x21C), (1, lambda a: 8), (2, lambda a: 0x194)],
                   nodes=_brake_wall_query_nodes, direct_fuzz=_brake_wall_query_point_fuzz),
    0x4E3F10: dict(name="CBrakeWall::QueryBox", popped=8, x87=False, thiscall=True, direct=True,
                   windows=[(0, lambda a: 0x21C), (1, lambda a: 0x10), (2, lambda a: 0x194)],
                   nodes=_brake_wall_query_nodes, direct_fuzz=_brake_wall_query_box_fuzz),
    0x4E4070: dict(name="CBrakeWall::Vf15C", popped=4, x87=False, thiscall=True, direct=True,
                   windows=[(0, lambda a: 0x21C), (1, lambda a: 0x10)],
                   nodes=_brake_wall_query_nodes, direct_fuzz=_brake_wall_vf15c_direct_fuzz),
    0x4E3810: dict(name="CBrakeWall::AddCount", popped=8, x87=False, thiscall=True, direct=True,
                   windows=[(1, lambda a: 0x400)], direct_fuzz=_brake_wall_add_count_direct_fuzz),
    # The fastcall's second register argument is unused; thiscall sets its ECX and stack arguments.
    0x4F2CE0: dict(name="E36SphereBox", popped=8, x87=False, thiscall=True, direct=True,
                   windows=[(0, lambda a: 0x300), (1, lambda a: 0x14), (2, lambda a: 4)],
                   direct_fuzz=_e36_sphere_box_direct_fuzz),
    0x4F2C30: dict(name="E36Get680", popped=0, x87=False, thiscall=True, direct=True,
                   windows=[(0, lambda a: 0x690)], direct_fuzz=_e36_get680_direct_fuzz),
    0x4F2C40: dict(name="E36Get68a", popped=0, x87=False, thiscall=True, direct=True,
                   windows=[(0, lambda a: 0x690)], direct_fuzz=_e36_get68a_direct_fuzz),
    0x4F2C50: dict(name="E36CollideInit", popped=0, x87=False, thiscall=True, direct=True,
                   windows=[(0, lambda a: 0x90)], direct_fuzz=_e36_collide_init_direct_fuzz),
    0x4F2CD0: dict(name="E36Nop", popped=0, x87=False, cdecl=True, direct=True,
                   windows=[], direct_fuzz=_e36_nop_direct_fuzz),
    0x4F3030: dict(name="E36ObbTest", popped=8, x87=False, thiscall=True, direct=True,
                   windows=[(0, lambda a: 0x300), (1, lambda a: 0x50), (2, lambda a: 4)],
                   direct_fuzz=_e36_obb_test_direct_fuzz),
    0x4F3950: dict(name="E36Collect", popped=12, x87=False, thiscall=True, direct=True,
                   windows=[(0, lambda a: 0x300), (1, lambda a: 0xD0), (2, lambda a: 4), (3, lambda a: 4)],
                   nodes=_e36_collect_nodes, dispatch=_e36_collect_dispatch, fuzz=_fuzz_e36_collect),
    0x4F3F80: dict(name="CCollisionEvent::Get94", popped=0, x87=True,
                   thiscall=True, direct=True, windows=[(0, lambda a: 0x98)],
                   direct_fuzz=_e37_get94_direct_fuzz),
    0x4F4070: dict(name="E37GetF7F0", popped=0, x87=False, cdecl=True,
                   direct=True, windows=[], nodes=_e37_get_f7f0_nodes,
                   direct_fuzz=_e37_get_f7f0_direct_fuzz),
    0x4F4090: dict(name="E37Zero4090", popped=0, x87=False, cdecl=True,
                   direct=True, windows=[], direct_fuzz=_e37_scalar_getter_direct_fuzz),
    0x4F40A0: dict(name="E37Nop40A0", popped=0, x87=False, cdecl=True,
                   direct=True, windows=[], direct_fuzz=_e37_scalar_getter_direct_fuzz),
    0x4F40B0: dict(name="E37Zero40B0", popped=0, x87=False, cdecl=True,
                   direct=True, windows=[], direct_fuzz=_e37_scalar_getter_direct_fuzz),
    0x4F40C0: dict(name="E37Get795E14a", popped=0, x87=True, cdecl=True,
                   direct=True, windows=[], direct_fuzz=_e37_scalar_getter_direct_fuzz),
    0x4F40D0: dict(name="E37Get795E14b", popped=0, x87=True, cdecl=True,
                   direct=True, windows=[], direct_fuzz=_e37_scalar_getter_direct_fuzz),
    0x4F40E0: dict(name="E37Get72AFCC", popped=0, x87=True, cdecl=True,
                   direct=True, windows=[], direct_fuzz=_e37_scalar_getter_direct_fuzz),
    0x4F3F90: dict(name="E37CmpInt", popped=8, x87=False, cdecl=True, direct=True,
                   windows=[(0, lambda a: 8), (1, lambda a: 8)],
                   direct_fuzz=_e37_cmp_int_direct_fuzz, branch=_e37_cmp_int_branch),
    0x4F3FB0: dict(name="E37CmpFloat", popped=8, x87=False, cdecl=True, direct=True,
                   windows=[(0, lambda a: 8), (1, lambda a: 8)],
                   direct_fuzz=_e37_cmp_float_direct_fuzz, branch=_e37_cmp_float_branch),
    0x4F4030: dict(name="E37FindInt", popped=12, x87=False, cdecl=True, direct=True,
                   windows=[(0, lambda a: max(1, a[1]) * 8)],
                   direct_fuzz=_e37_find_int_direct_fuzz),
    0x4F4080: dict(name="E37Init70Obj::Init70", popped=4, x87=False, thiscall=True, direct=True,
                   windows=[(0, lambda a: 0x80), (1, lambda a: 16)],
                   direct_fuzz=_e37_init_70_direct_fuzz),
    0x4F4190: dict(name="E37EvOps::PlaneScale", popped=4, x87=False, thiscall=True, direct=True,
                   windows=[(0, lambda a: 0xD0), (1, lambda a: 16)],
                   direct_fuzz=_e37_plane_scale_direct_fuzz),
    0x4F5890: dict(name="E37GridOps::T68A", popped=0, x87=False, thiscall=True, direct=True,
                   windows=[(0, lambda a: 0x690)], direct_fuzz=_e37_elem_getter_direct_fuzz),
    0x4F58A0: dict(name="E37GridOps::AtE8", popped=4, x87=False, thiscall=True, direct=True,
                   windows=[(0, lambda a: 0x690)], direct_fuzz=_e37_elem_index_direct_fuzz),
    0x4F58B0: dict(name="E37GridOps::AtE10", popped=4, x87=False, thiscall=True, direct=True,
                   windows=[(0, lambda a: 0x690)], direct_fuzz=_e37_elem_index_direct_fuzz),
    0x4F58C0: dict(name="E37GridOps::Idx7", popped=4, x87=False, thiscall=True, direct=True,
                   windows=[(0, lambda a: 0x690)], direct_fuzz=_e37_elem_index_direct_fuzz),
    0x4F58E0: dict(name="E37GridOps::Get684", popped=0, x87=False, thiscall=True, direct=True,
                   windows=[(0, lambda a: 0x690)], direct_fuzz=_e37_elem_getter_direct_fuzz),
    0x4F58F0: dict(name="E37GridOps::Set68A", popped=4, x87=False, thiscall=True, direct=True,
                   windows=[(0, lambda a: 0x690)], direct_fuzz=_e37_elem_set_direct_fuzz),
    0x4F5920: dict(name="E37GridOps::T158_2", popped=0, x87=False, thiscall=True, direct=True,
                   windows=[(0, lambda a: 0x690)], direct_fuzz=_e37_elem_getter_direct_fuzz),
    0x4F5930: dict(name="E37GridOps::T158_4", popped=0, x87=False, thiscall=True, direct=True,
                   windows=[(0, lambda a: 0x690)], direct_fuzz=_e37_elem_getter_direct_fuzz),
    0x4F5940: dict(name="E37GridOps::Get30", popped=0, x87=False, thiscall=True, direct=True,
                   windows=[(0, lambda a: 0x690)], direct_fuzz=_e37_elem_getter_direct_fuzz),
    0x4F5950: dict(name="E37GridOps::Get04", popped=0, x87=False, thiscall=True, direct=True,
                   windows=[(0, lambda a: 0x30)], direct_fuzz=_e37_elem_small_getter_direct_fuzz),
    0x4F5960: dict(name="E37GridOps::Set04", popped=4, x87=False, thiscall=True, direct=True,
                   windows=[(0, lambda a: 0x30)], direct_fuzz=_e37_elem_small_set_direct_fuzz),
    0x4F5970: dict(name="E37GridOps::Get00", popped=0, x87=False, thiscall=True, direct=True,
                   windows=[(0, lambda a: 0x30)], direct_fuzz=_e37_elem_small_getter_direct_fuzz),
    0x4F5980: dict(name="E37GridOps::T04_2", popped=0, x87=False, thiscall=True, direct=True,
                   windows=[(0, lambda a: 0x30)], direct_fuzz=_e37_elem_small_getter_direct_fuzz),
    0x4F5990: dict(name="E37GridOps::Valid5990", popped=0, x87=False, thiscall=True, direct=True,
                   windows=[(0, lambda a: 0x30)], direct_fuzz=_e37_elem_valid_5990_direct_fuzz),
    0x4F59B0: dict(name="E37GridOps::Get08", popped=0, x87=False, thiscall=True, direct=True,
                   windows=[(0, lambda a: 0x30)], direct_fuzz=_e37_elem_small_getter_direct_fuzz),
    0x4F59C0: dict(name="E37GridOps::Get0Cw", popped=0, x87=False, thiscall=True, direct=True,
                   windows=[(0, lambda a: 0x30)], direct_fuzz=_e37_elem_small_getter_direct_fuzz),
    0x4F59D0: dict(name="E37GridOps::Get10", popped=0, x87=False, thiscall=True, direct=True,
                   windows=[(0, lambda a: 0x30)], direct_fuzz=_e37_elem_small_getter_direct_fuzz),
    0x4F59E0: dict(name="E37GridOps::Get0Ew", popped=0, x87=False, thiscall=True, direct=True,
                   windows=[(0, lambda a: 0x30)], direct_fuzz=_e37_elem_small_getter_direct_fuzz),
    0x4F59F0: dict(name="E37GridOps::Get14", popped=0, x87=False, thiscall=True, direct=True,
                   windows=[(0, lambda a: 0x30)], direct_fuzz=_e37_elem_small_getter_direct_fuzz),
    0x4F5A00: dict(name="E37GridOps::Get18", popped=0, x87=False, thiscall=True, direct=True,
                   windows=[(0, lambda a: 0x30)], direct_fuzz=_e37_elem_small_getter_direct_fuzz),
    0x4F5A10: dict(name="E37GridOps::Get1C", popped=0, x87=False, thiscall=True, direct=True,
                   windows=[(0, lambda a: 0x30)], direct_fuzz=_e37_elem_small_getter_direct_fuzz),
    0x4F5A20: dict(name="E37GridOps::Get20w", popped=0, x87=False, thiscall=True, direct=True,
                   windows=[(0, lambda a: 0x30)], direct_fuzz=_e37_elem_small_getter_direct_fuzz),
    0x4F5A30: dict(name="E37GridOps::Get22w", popped=0, x87=False, thiscall=True, direct=True,
                   windows=[(0, lambda a: 0x30)], direct_fuzz=_e37_elem_small_getter_direct_fuzz),
    0x4F5A40: dict(name="E37GridOps::Get24", popped=0, x87=False, thiscall=True, direct=True,
                   windows=[(0, lambda a: 0x30)], direct_fuzz=_e37_elem_small_getter_direct_fuzz),
    0x4F5A50: dict(name="E37GridOps::Get28", popped=0, x87=False, thiscall=True, direct=True,
                   windows=[(0, lambda a: 0x30)], direct_fuzz=_e37_elem_small_getter_direct_fuzz),
    0x4F5A60: dict(name="E37GridOps::Get2C", popped=0, x87=False, thiscall=True, direct=True,
                   windows=[(0, lambda a: 0x30)], direct_fuzz=_e37_elem_small_getter_direct_fuzz),
    0x4F5A70: dict(name="E37Zero5A70", popped=0, x87=False, cdecl=True, direct=True,
                   windows=[], nodes=_e37_elem_global_nodes, direct_fuzz=_e37_elem_global_direct_fuzz),
    0x4F5A80: dict(name="E37Get5A80", popped=0, x87=False, cdecl=True, direct=True,
                   windows=[], nodes=_e37_elem_global_nodes, direct_fuzz=_e37_elem_global_direct_fuzz),
    0x4F5CF0: dict(name="E37Nop5CF0", popped=0, x87=False, cdecl=True, direct=True,
                   windows=[], direct_fuzz=_e37_nop_direct_fuzz),
    0x4F5D00: dict(name="E37Compose9", popped=36, x87=False, cdecl=True, direct=True,
                   windows=[(i, lambda a: 16) for i in range(9)],
                   direct_fuzz=_e37_compose_9_direct_fuzz),
    0x4F5A90: dict(name="E37Tri5A90", popped=24, x87=False, cdecl=True, direct=True,
                   windows=[(i, (lambda a: 16) if i < 5 else (lambda a: 32)) for i in range(6)],
                   direct_fuzz=_e37_tri_5a90_direct_fuzz),
    0x4F56C0: dict(name="E37Run56C0", popped=24, x87=False, cdecl=True, direct=True,
                   windows=[(i, (lambda a: 16) if i < 5 else (lambda a: 4)) for i in range(6)],
                   direct_fuzz=_e37_run_56c0_direct_fuzz),
    0x4F5F20: dict(name="E37GridOps::Elem5F20", popped=12, x87=True, thiscall=True, direct=True,
                   windows=[(0, lambda a: 0x1000), (2, lambda a: 16), (3, lambda a: 16)],
                   nodes=_e37_elem_5f20_nodes, direct_fuzz=_e37_elem_5f20_direct_fuzz),
    0x4FCC70: dict(name="E38Vec::Scale", popped=4, x87=False, thiscall=True, direct=True,
                   windows=[(0, lambda a: 16)], direct_fuzz=_e38_scale_direct_fuzz),
    0x4FCCB0: dict(name="E38Vec::M_CCB0", popped=12, x87=False, thiscall=True, direct=True,
                   windows=[(i, lambda a: 16) for i in range(4)],
                   direct_fuzz=_e38_add_sub_direct_fuzz),
    0x4FCD10: dict(name="E38Vec::M_CD10", popped=8, x87=False, thiscall=True, direct=True,
                   windows=[(i, lambda a: 16) for i in range(3)],
                   direct_fuzz=_e38_binary_vec_direct_fuzz),
    0x4FCDD0: dict(name="E38Vec::M_CDD0", popped=8, x87=False, thiscall=True, direct=True,
                   windows=[(i, lambda a: 16) for i in range(3)],
                   direct_fuzz=_e38_binary_vec_direct_fuzz),
    0x4FCD60: dict(name="E38Vec::M_CD60", popped=12, x87=True, thiscall=True, direct=True,
                   windows=[(i, lambda a: 16) for i in range(4)],
                   direct_fuzz=_e38_three_vecs_direct_fuzz),
    0x4FCEE0: dict(name="E38Vec::M_CEE0", popped=4, x87=True, thiscall=True, direct=True,
                   windows=[(i, lambda a: 16) for i in range(2)],
                   direct_fuzz=_e38_dot_direct_fuzz),
    0x4FCF30: dict(name="E38Vec::M_CF30", popped=8, x87=False, thiscall=True, direct=True,
                   windows=[(i, lambda a: 16) for i in range(3)],
                   direct_fuzz=_e38_binary_vec_direct_fuzz),
    0x4FCFA0: dict(name="E38Vec::M_CFA0", popped=12, x87=True, thiscall=True, direct=True,
                   windows=[(i, lambda a: 16) for i in range(4)],
                   direct_fuzz=_e38_three_vecs_direct_fuzz),
    0x4FD030: dict(name="E38Vec::M_D030", popped=8, x87=False, thiscall=True, direct=True,
                   windows=[(0, lambda a: 16), (2, lambda a: 16)],
                   direct_fuzz=_e38_scale_to_direct_fuzz),
    0x4FD0F0: dict(name="E38_004FD0F0", popped=0, x87=False, thiscall=True, direct=True,
                   windows=[(0, lambda a: 16)],
                   direct_fuzz=_e38_normalize_direct_fuzz),
    0x4FD230: dict(name="E38Ops::M_D230", popped=8, x87=False, thiscall=True, direct=True,
                   windows=[(0, lambda a: 24), (1, lambda a: 12), (2, lambda a: 12)],
                   direct_fuzz=_e38_sat_direct_fuzz),
    0x4FD2C0: dict(name="E38Ops::M_D2C0", popped=8, x87=False, thiscall=True, direct=True,
                   windows=[(0, lambda a: 24), (1, lambda a: 12), (2, lambda a: 12)],
                   direct_fuzz=_e38_sat_direct_fuzz),
    0x4FD480: dict(name="E38Ops::M_D480", popped=24, x87=False, thiscall=True, direct=True,
                   windows=[(i, lambda a: 16) for i in range(6)] + [(6, lambda a: 44)],
                   direct_fuzz=_e38_d480_direct_fuzz),
    0x4FAC50: dict(name="E38Ops::M_AC50", popped=16, x87=False, thiscall=True, direct=True,
                   windows=[(0, lambda a: 0xC0), (1, lambda a: 16), (2, lambda a: 16),
                            (3, lambda a: 16), (4, lambda a: 4)],
                   nodes=_e38_mesh_ac50_nodes, direct_fuzz=_e38_mesh_ac50_direct_fuzz),
    0x4FAD10: dict(name="E38_004FAD10", popped=0, x87=False, thiscall=True, direct=True,
                   windows=[(0, lambda a: 0x804)], direct_fuzz=_e38_clear_4fad10_direct_fuzz),
    0x4FB330: dict(name="CCollisionSolver::CDef::Unk20", popped=0, x87=False,
                   thiscall=True, direct=True, windows=[],
                   direct_fuzz=_e38_solver_def_table_direct_fuzz),
    0x4FB720: dict(name="E38_004FB720", popped=8, x87=False, direct=True,
                   windows=[(0, lambda a: 0xE8), (1, lambda a: 4)],
                   nodes=_e38_solver_4fb720_nodes,
                   dispatch=lambda _mu, _a: [(0x4F3E80, 0), (0x520940, 4)],
                   direct_fuzz=_e38_solver_4fb720_direct_fuzz,
                   branch=_e38_solver_4fb720_branch),
    0x4F3E80: dict(name="CE36SubBlock::Init", popped=0, x87=False, thiscall=True,
                   external=True, windows=[(0, lambda a: 12)]),
    0x520940: dict(name="CMaterialManager::Tag520940", popped=4, x87=False, thiscall=True,
                   external=True, windows=[]),
    0x4F4B20: dict(name="E37Test4B20", popped=0, x87=False, thiscall=True, direct=True,
                   windows=[(0, lambda a: 0xE8)], direct_fuzz=_e37_test_4b20_direct_fuzz),
    0x4F4A20: dict(name="E37Refresh4A20", popped=0, x87=False, thiscall=True, direct=True,
                   windows=[(0, lambda a: 0xE8)], nodes=_e37_refresh_4a20_nodes,
                   direct_fuzz=_e37_refresh_4a20_direct_fuzz,
                   branch=_e37_refresh_4a20_branch),
    0x4F4200: dict(name="E37EvOps::Copy80", popped=4, x87=False, thiscall=True, direct=True,
                   windows=[(0, lambda a: 0x8C), (1, lambda a: 12)],
                   direct_fuzz=_e37_copy_80_direct_fuzz),
    0x4F4570: dict(name="E37EvOps::Cmp15", popped=4, x87=False, thiscall=True, direct=True,
                   windows=[(0, lambda a: 0x3C), (1, lambda a: 0x3C)],
                   direct_fuzz=_e37_cmp_15_direct_fuzz),
    0x4F4B40: dict(name="E37Zero3", popped=0, x87=False, thiscall=True, direct=True,
                   windows=[(0, lambda a: 12)], direct_fuzz=_e37_zero3_direct_fuzz),
    0x4F4B50: dict(name="E37First", popped=0, x87=False, thiscall=True, direct=True,
                   windows=[(0, lambda a: 4)], direct_fuzz=_e37_first_direct_fuzz),
    0x4F4B60: dict(name="E37FirstTwin", popped=0, x87=False, thiscall=True, direct=True,
                   windows=[(0, lambda a: 4)], direct_fuzz=_e37_first_direct_fuzz),
    0x4F4B70: dict(name="E37At8", popped=0, x87=False, thiscall=True, direct=True,
                   windows=[(0, lambda a: 12)], direct_fuzz=_e37_at8_direct_fuzz),
    0x4F4B80: dict(name="E37Zero4", popped=0, x87=False, thiscall=True, direct=True,
                   windows=[(0, lambda a: 16)], direct_fuzz=_e37_zero4_direct_fuzz),
    0x4F4B90: dict(name="E37ListOps::Insert", popped=4, x87=False, thiscall=True, direct=True,
                   windows=[(0, lambda a: 0x0C), (1, lambda a: 0x18)],
                   nodes=_e37_list_insert_nodes, direct_fuzz=_e37_list_insert_direct_fuzz),
    0x4F4BC0: dict(name="E37Unlink", popped=0, x87=False, thiscall=True, direct=True,
                   windows=[(0, lambda a: 12)], nodes=_e37_unlink_nodes,
                   direct_fuzz=_e37_unlink_direct_fuzz),
    0x4F4C30: dict(name="E37PushFront", popped=0, stack_words=1, x87=False, cdecl=True, direct=True,
                   windows=[(0, lambda a: 0x18)], nodes=_e37_push_front_nodes,
                   direct_fuzz=_e37_push_front_direct_fuzz, branch=_e37_push_front_branch),
    0x4F52C0: dict(name="CCollisionEvent::DtorBody", popped=0, x87=False, thiscall=True, direct=True,
                   windows=[(0, lambda a: 0x24)], nodes=_e37_event_dtor_nodes,
                   direct_fuzz=_e37_event_dtor_direct_fuzz, branch=_e37_event_dtor_branch),
    0x4F52F0: dict(name="CCollisionEvent::Vf1Cx", popped=4, x87=False, thiscall=True, direct=True,
                   windows=[(0, lambda a: 0x24)], nodes=_e37_event_vf1cx_nodes,
                   direct_fuzz=_e37_event_vf1cx_direct_fuzz, branch=_e37_event_vf1cx_branch),
    0x4F4EC0: dict(name="E37ListOps::Drain", popped=4, x87=False, thiscall=True, direct=True,
                   windows=[(0, lambda a: 12)], nodes=_e37_drain_nodes,
                   direct_fuzz=_e37_drain_direct_fuzz, branch=_e37_drain_branch),
    0x4F4F40: dict(name="CCollisionList::Vf08x", popped=0, x87=False, thiscall=True, direct=True,
                   windows=[(0, lambda a: 0x10)], nodes=_e37_drain_vf08x_nodes,
                   direct_fuzz=_e37_drain_vf08x_direct_fuzz, branch=_e37_drain_vf08x_branch),
    0x4F4F70: dict(name="E37Drain4F70", popped=0, x87=False, thiscall=True, direct=True,
                   windows=[(0, lambda a: 0x10)], nodes=_e37_drain_nodes,
                   direct_fuzz=_e37_drain_4f70_direct_fuzz, branch=_e37_drain_4f70_branch),
    0x4F4C70: dict(name="E37Rate4C70", popped=0, x87=True, thiscall=True, direct=True,
                   windows=[(0, lambda a: 0x10)], nodes=_e37_rate_nodes,
                   direct_fuzz=_e37_rate_direct_fuzz),
    0x4F4F50: dict(name="E37Release4F50", popped=0, x87=False, thiscall=True, direct=True,
                   windows=[(0, lambda a: 12)], nodes=_e37_release_owner_nodes,
                   direct_fuzz=_e37_release_owner_direct_fuzz),
    0x4F4F60: dict(name="E37Release4F60", popped=0, x87=False, thiscall=True, direct=True,
                   windows=[(0, lambda a: 12)], nodes=_e37_release_owner_nodes,
                   direct_fuzz=_e37_release_owner_direct_fuzz),
    0x4F4EA0: dict(name="E37ListOps::Release", popped=4, x87=False, thiscall=True, direct=True,
                   windows=[(0, lambda a: 12), (1, lambda a: 12)], nodes=_e37_release_nodes,
                   direct_fuzz=_e37_release_direct_fuzz),
    0x4F5470: dict(name="E37ListOps::Set1C", popped=4, x87=False, thiscall=True, direct=True,
                   windows=[(0, lambda a: 0x20)], direct_fuzz=_e37_set1c_direct_fuzz),
    0x4F5480: dict(name="E37Vec16::Min", popped=4, x87=False, thiscall=True, direct=True,
                   windows=[(0, lambda a: 16), (1, lambda a: 16)],
                   direct_fuzz=_e37_vec16_minmax_direct_fuzz),
    0x4F54B0: dict(name="E37Vec16::Max", popped=4, x87=False, thiscall=True, direct=True,
                   windows=[(0, lambda a: 16), (1, lambda a: 16)],
                   direct_fuzz=_e37_vec16_minmax_direct_fuzz),
    0x4F54E0: dict(name="E37AllNonNeg", popped=0, x87=False, thiscall=True, direct=True,
                   windows=[(0, lambda a: 12)], direct_fuzz=_e37_sign_vector_direct_fuzz),
    0x4F5510: dict(name="E37AllNonPos", popped=0, x87=False, thiscall=True, direct=True,
                   windows=[(0, lambda a: 12)], direct_fuzz=_e37_sign_vector_direct_fuzz),
    0x4F5530: dict(name="E37VecSet::DotX", popped=8, x87=True, thiscall=True, direct=True,
                   windows=[(0, lambda a: 12), (1, lambda a: 12), (2, lambda a: 12)],
                   direct_fuzz=_e37_dot_x_direct_fuzz),
    0x4F5630: dict(name="E37VecSet::Store5630", popped=12, x87=False, thiscall=True, direct=True,
                   windows=[(0, lambda a: 12), (1, lambda a: 12), (2, lambda a: 12), (3, lambda a: 12)],
                   direct_fuzz=_e37_store_5630_direct_fuzz),
    0x5240C0: dict(name="E45_M40C0", popped=0, stack_words=3, cdecl=True, x87=False, external=True,
                   windows=[(0, lambda a: 8), (1, lambda a: 8), (2, lambda a: 16)],
                   nodes=_e36_query_nodes),
    0x51B4C0: dict(name="E33Mat34::ComposeA", popped=12, x87=False, thiscall=True, external=True,
                   windows=[(1, lambda a: 0x50), (2, lambda a: 0x40)],
                   after=_e36_compose_after),
    0x51B560: dict(name="E43PairWalk::MB560", popped=12, x87=False, thiscall=True, external=True,
                   windows=[(1, lambda a: 0x10), (2, lambda a: 0x40), (3, lambda a: 0x10)]),
    0x70BEC2: dict(name="operator new (CRT)", popped=4, x87=False, cdecl=True, external=True,
                   windows=[]),
    0x4F4F10: dict(name="E36AllocEvent", popped=4, x87=False, cdecl=True, direct=True,
                   external=True, windows=[], nodes=_e36_alloc_event_nodes,
                   direct_fuzz=_e36_alloc_event_direct_fuzz, branch=_e36_alloc_event_branch),
    0x4F50E0: dict(name="CCollisionEvent::InitFull", popped=24, x87=False,
                   thiscall=True, external=True,
                   windows=[(0, lambda a: 0xF0), (5, lambda a: 0x40)]),
    0x4F7090: dict(name="E37WallSync", popped=12, x87=False, thiscall=True,
                   external=True,
                   windows=[(0, lambda a: 0x6B0), (2, lambda a: 0x10), (3, lambda a: 0x10)]),
    0x4F5CA0: dict(name="E36SelectTable", popped=4, x87=False, cdecl=True, direct=True,
                   windows=[(0, lambda a: 0x48)], nodes=_select_table_nodes,
                   direct_args=_select_table_direct_args, fuzz=_fuzz_select_table,
                   branch=_select_table_branch),
    0x519C00: dict(name="CEntity::Vf30", popped=0, x87=False, thiscall=True,
                   external=True, windows=[]),
    0x519E60: dict(name="CEntity::Vf44", popped=4, x87=False, thiscall=True,
                   external=True, windows=[(0, lambda a: 0xD0)]),
    0x51B6C0: dict(name="E33Mat34::FetchX", popped=8, x87=False, thiscall=True,
                   external=True,
                   windows=[(0, lambda a: 8), (0, lambda a: 0x10, 0), (1, lambda a: 0x10)]),
    0x51A4B0: dict(name="CEntity::Vf40", popped=0, x87=False, thiscall=True,
                   external=True, windows=[(0, lambda a: 0x70)]),
    # MinMaxA is thiscall with three stack pointers; it calls Min and Max directly.
    0x4E0AE0: dict(name="E33Vec4::MinMaxA", popped=12, x87=False, thiscall=True, direct=True,
                   windows=[(0, lambda a: 0x20), (1, lambda a: 0x20), (2, lambda a: 0x10), (3, lambda a: 0x10)]),
    0x4E0750: dict(name="E33Vec4::Min", popped=8, x87=False, thiscall=True,
                   windows=[(0, lambda a: 0x10), (1, lambda a: 0x10), (2, lambda a: 0x10)]),
    0x4E0780: dict(name="E33Vec4::Max", popped=8, x87=False, thiscall=True,
                   windows=[(0, lambda a: 0x10), (1, lambda a: 0x10), (2, lambda a: 0x10)]),
    # The original reads the output pointer at [ebp+8] and the scale at [ebp+0xc].
    0x4E0700: dict(name="E33Vec4::Scaled", popped=8, x87=False, thiscall=True,
                   windows=[(0, lambda a: 0x10), (1, lambda a: 0x10)]),
    0x4E07B0: dict(name="E33Vec4::AllBelowZero", popped=0, x87=False, thiscall=True,
                   windows=[(0, lambda a: 0x0c)], branch=_all_below_zero, fuzz=_fuzz_all_below_zero),
    # BoxHit is cdecl with caller cleanup. The four 16-byte vectors are read-only inputs.
    0x4E0E40: dict(name="E33Vec4::BoxHit", popped=16, x87=False, cdecl=True,
                   windows=[(0, lambda a: 0x10), (1, lambda a: 0x10), (2, lambda a: 0x10), (3, lambda a: 0x10)],
                   branch=_box_hit, fuzz=_fuzz_box_hit),
    0x4E11B0: dict(name="CBody::Vf10C", popped=0, x87=False, thiscall=True,
                   windows=[(0, lambda a: 0x68)]),
    0x4E11C0: dict(name="CBody::Vf110", popped=0, x87=False, thiscall=True,
                   windows=[(0, lambda a: 0x68), (0, lambda a: 0x6b0, 0x64)]),
    0x4E11E0: dict(name="CBody::Vf114", popped=0, x87=False, thiscall=True,
                   windows=[(0, lambda a: 0x68)]),
    0x4E11F0: dict(name="CBody::Vf118", popped=0, x87=False, thiscall=True,
                   windows=[(0, lambda a: 0x68)]),
    0x4E1200: dict(name="CBody::Vf11C", popped=8, x87=False, thiscall=True,
                   windows=[(0, lambda a: 0xa0), (1, lambda a: 0x10), (2, lambda a: 0x10)]),
    0x4E0820: dict(name="CColGrid::Flag68A", popped=0, x87=False, thiscall=True,
                   windows=[(0, lambda a: 0x6b0)]),
    0x4E0830: dict(name="CColGrid::Flag68F", popped=0, x87=False, thiscall=True,
                   windows=[(0, lambda a: 0x6b0)]),
    0x4E0A20: dict(name="CColGrid::ReadClear", popped=8, x87=False, thiscall=True,
                   windows=[(0, lambda a: 0x6b0), (1, lambda a: 4), (2, lambda a: 4)],
                   direct_args=_read_clear_direct_args),
    0x4E0A50: dict(name="CColGrid::StoreVec", popped=4, x87=False, thiscall=True,
                   windows=[(0, lambda a: 0x30), (1, lambda a: 0x10)], direct_args=_store_vec_direct_args),
    0x4E0A60: dict(name="CColGrid::EdgeAt", popped=4, x87=False, thiscall=True,
                   windows=[(0, lambda a: 0x50)], direct_args=_edge_at_direct_args),
    0x4E0A70: dict(name="CColGrid::Flag68D", popped=0, x87=False, thiscall=True,
                   windows=[(0, lambda a: 0x6b0)]),
    0x4E0A80: dict(name="CColGrid::Flag68B", popped=0, x87=False, thiscall=True,
                   windows=[(0, lambda a: 0x6b0)]),
    0x4E0A90: dict(name="CColGrid::EdgeForA", popped=4, x87=False, thiscall=True,
                   windows=[(0, lambda a: 0x6b0)], direct_args=_edge_pair_direct_args),
    0x4E0AB0: dict(name="CColGrid::EdgeForB", popped=4, x87=False, thiscall=True,
                   windows=[(0, lambda a: 0x6b0)], direct_args=_edge_pair_direct_args),
    0x4E0AD0: dict(name="CColGrid::Flag68C", popped=0, x87=False, thiscall=True,
                   windows=[(0, lambda a: 0x6b0)]),
    0x4E0840: dict(name="CColGrid::SampleA", popped=16, x87=False, thiscall=True,
                   branch=_sample_a_branch,
                   windows=[(0, lambda a: 0x6b0), (3, lambda a: 4), (4, lambda a: 4)],
                   nodes=_sample_a_nodes, direct_args=_sample_a_direct_args),
    0x4F6550: dict(name="E38RaySeeds", popped=0, x87=False, thiscall=True, direct=True,
                   windows=[(0, lambda a: 0x6B0)], direct_fuzz=_ray_object_direct_fuzz),
    0x4F65B0: dict(name="E37GridOps::AddEdge", popped=4, x87=False, thiscall=True, direct=True,
                   windows=[(0, lambda a: 0x6B0), (1, lambda a: 16)], direct_fuzz=_grid_add_edge_direct_fuzz),
    0x4F6740: dict(name="E38RayClear", popped=0, x87=False, thiscall=True, direct=True,
                   windows=[(0, lambda a: 0x6B0)], direct_fuzz=_ray_object_direct_fuzz),
    # E09Vec4::Clear (thiscall, `this` returned): four zero floats.
    0x465D80: dict(name="E09Vec4::Clear", popped=0, x87=False, thiscall=True, external=True,
                   windows=[(0, lambda a: 0x10)]),
    # M_D760X (0x4fd760): the nearest face of the records in the list (the grid's own at +0x1014 when the argument is null).
    0x4FD760: dict(name="M_D760X", popped=16, x87=False, thiscall=True, direct=True,
                   windows=[(0, lambda a: 0x1040), (1, lambda a: 0x10), (2, lambda a: 0x10), (3, lambda a: 0x40)],
                   nodes=_m_d760x_nodes, direct_fuzz=_m_d760x_direct_fuzz),
    # The sample table lookup M_D760X makes on each candidate face (thiscall on the table, three stack words).
    0x520240: dict(name="FUN_00520240", popped=12, x87=False, thiscall=True, external=True,
                   windows=[(0, lambda a: 0x808)]),
    # E38NodeWalk (0x4fdd60): stdcall, the query box, the root and the output (count, then leaves).
    0x4FDD60: dict(name="E38NodeWalk", popped=12, x87=False, direct=True,
                   windows=[(0, lambda a: 0x18), (2, lambda a: 0x100)], nodes=_node_walk_nodes,
                   direct_fuzz=_node_walk_direct_fuzz),
    # The E38 solver passes (direct runs on _e38_solver_direct_fuzz's solver) and the calls they leave to the host.
    0x4FB7F0: _e38_solver_spec("E38_004FB7F0", []),
    0x4FBC20: _e38_solver_spec("E38_004FBC20", [(0x5297D0, 0)]),
    0x4FC0B0: _e38_solver_spec("CCollisionSolver::Vf08", []),
    0x4FC590: _e38_solver_spec("E38_004FC590X", [(0x5297D0, 0), (0x4FBD30, 12), (0x4FB890, 0)],
                                 _e38_correction_branch),
    0x4FCA20: _e38_solver_spec("E38_004FCA20", [(0x5297D0, 0), (0x5293D0, 0), (0x5293E0, 0), (0x4FB820, 0),
                                                (0x4FC1A0, 4)], _e38_due_branch),
    0x5297D0: dict(name="E47Solid::Vf79x", popped=0, x87=True, thiscall=True, external=True, windows=[]),
    0x5293D0: dict(name="CSolid slot 0x80", popped=0, x87=False, thiscall=True, external=True, windows=[]),
    0x5293E0: dict(name="CSolid slot 0x8c", popped=0, x87=False, thiscall=True, external=True, windows=[]),
    0x4FB820: dict(name="E38_004FB820", popped=0, x87=False, thiscall=True, external=True, windows=[]),
    0x4FBD30: dict(name="E38Ops::M_BD30", popped=12, x87=False, thiscall=True, external=True,
                   windows=[(0, lambda a: 0x20 + 0x310 * 2)], after=lambda mu, a, _e: [(a[1] + 0x274, 4)]),
    0x4FB890: dict(name="E38_004FB890", popped=0, x87=False, thiscall=True, external=True,
                   windows=[(0, lambda a: 0x90)], after=_e38_row_body),
    0x4FC1A0: dict(name="E38Ops::M_C1A0", popped=4, x87=True, thiscall=True, external=True,
                   windows=[(1, lambda a: 0x100)], after=_e38_event_bodies, nodes=_e38_impulse_nodes,
                   direct_fuzz=_e38_impulse_direct_fuzz),
    # M_C1A0's calls (direct runs of 0x4fc1a0): the relative velocity, the two responses and the impulse's emit.
    0x4F40F0: dict(name="E37EvOps::PlaneDiff", popped=4, x87=False, thiscall=True, external=True,
                   windows=[(1, lambda a: 0x10)]),
    0x4F4390: dict(name="E37EvOps::Query4390", popped=0x14, x87=False, thiscall=True, external=True,
                   windows=[(1, lambda a: 0x10), (2, lambda a: 0x10), (4, lambda a: 4), (5, lambda a: 4)]),
    0x4F4DC0: dict(name="E37EvOps::Emit4DC0", popped=0x10, x87=False, thiscall=True, external=True,
                   windows=[(3, lambda a: 0x10), (4, lambda a: 0x10)], after=_e38_event_bodies_of_this),
}
