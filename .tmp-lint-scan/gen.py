import json, sys, copy

p = '.rust-migration/migration-state.json'
n = int(sys.argv[1])
ready = int(sys.argv[2])

s = json.load(open(p))
tpl = list(s['modules'].values())[0]


def mk(status, blocked_by=None, pre=None):
    m = copy.deepcopy(tpl)
    m['status'] = status
    m.pop('member_files', None)
    m['composite_kind'] = None
    m['blocked_by'] = blocked_by
    m['pre_blocked_status'] = pre
    return m


mods = {'file:dep.ts': mk('done')}
for i in range(ready):
    mods['blocked:%d' % i] = mk('blocked', ['file:dep.ts'], 'translating')
for i in range(n - ready - 1):
    mods['pending:%d' % i] = mk('pending')
s['modules'] = mods
json.dump(s, open(p, 'w'))
print('modules=%d ready=%d' % (len(mods), ready))
