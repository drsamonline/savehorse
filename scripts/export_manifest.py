"""Export the operation manifest from the Rust registry to JSON.

The registry is the single source of truth; this is a build-time snapshot for
the static demo page, which cannot call the Rust core.
"""
import json, re, sys

src = open('crates/savehorse-registry/src/lib.rs').read()

PARAM_FIELDS = re.compile(
    r'name: "([^"]+)",\s*'
    r'kind: ParamKind::(\w+),\s*'
    r'positional: (\w+),\s*'
    r'required: (\w+),\s*'
    r'help: "((?:[^"\\]|\\.)*)",\s*'
    r'options: &\[([^\]]*)\]')

def to_param(fields):
    name, kind, positional, required, help_, options = fields
    return {
        "name": name, "kind": kind,
        "positional": positional == "true",
        "required": required == "true",
        "help": help_.replace('\\"', '"'),
        "options": re.findall(r'"([^"]+)"', options),
    }

# Every `Param { ... }` block, by byte offset.
inline = {}
for m in re.finditer(r'Param \{\s*(.*?)\s*\}', src, re.S):
    f = PARAM_FIELDS.search(m.group(1))
    if f:
        inline[m.start()] = to_param(f.groups())

# Named single-parameter constants.
named = {}
for m in re.finditer(r'const (P_[A-Z_0-9]+): Param = Param \{\s*(.*?)\s*\}', src, re.S):
    f = PARAM_FIELDS.search(m.group(2))
    if f:
        named[m.group(1)] = to_param(f.groups())

# Parameter sets, with their bodies.
sets = {}
for m in re.finditer(r'const ([A-Z_0-9]+): &\[Param\] = &\[(.*?)\];', src, re.S):
    sets[m.group(1)] = (m.start(2), m.group(2))

def resolve(symbol):
    if symbol == '&[]' or symbol not in sets:
        return []
    body_start, body = sets[symbol]
    out = []
    # Inline Param blocks inside this body, in order.
    for pos, param in sorted(inline.items()):
        if body_start <= pos < body_start + len(body):
            out.append((pos, param))
    # Named constants referenced by identifier, in order.
    for m in re.finditer(r'\b(P_[A-Z_0-9]+)\b', body):
        if m.group(1) in named:
            out.append((body_start + m.start(), named[m.group(1)]))
    out.sort(key=lambda pair: pair[0])
    # De-duplicate by identity of position.
    seen, result = set(), []
    for pos, param in out:
        if pos in seen:
            continue
        seen.add(pos)
        result.append(param)
    return result

modules = []
for m in re.finditer(
    r'Module \{ name: "(\w+)",\s*title: "([^"]+)",\s*job: "([^"]+)",\s*sources: "([^"]+)" \}', src):
    name, title, job, sources = m.groups()
    modules.append({"name": name, "title": title, "job": job, "sources": sources, "ops": []})
by_name = {m["name"]: m for m in modules}

danger_map = {"Safe": "safe", "Destructive": "destructive", "Irreversible": "irreversible"}
status_map = {"Implemented": "implemented", "Planned": "planned"}

for m in re.finditer(
    r'Op \{ id: "([\w.]+)",\s*module: "(\w+)",\s*name: "(\w+)",\s*'
    r'summary: "((?:[^"\\]|\\.)*)",\s*danger: Danger::(\w+),\s*'
    r'status: Status::(\w+),\s*params: ([^\s}]+) \}', src):
    oid, module, name, summary, danger, status, params = m.groups()
    if module not in by_name:
        continue
    by_name[module]["ops"].append({
        "id": oid,
        "name": name,
        "summary": summary.replace('\\"', '"'),
        "danger": danger_map[danger],
        "status": status_map[status],
        "params": resolve(params),
    })

total = sum(len(m["ops"]) for m in modules)
implemented = sum(1 for m in modules for o in m["ops"] if o["status"] == "implemented")
with_params = sum(1 for m in modules for o in m["ops"] if o["params"])
print(f"modules {len(modules)}  ops {total}  implemented {implemented}  with params {with_params}", file=sys.stderr)
json.dump(modules, open(sys.argv[1], 'w'), indent=1)
