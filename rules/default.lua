-- Peabiru — regras de diagnóstico padrão (ARCHITECTURE.md §5, VISION §22–23).
--
-- Cada arquivo de regras roda uma vez por trace, num ambiente próprio dentro
-- de um sandbox Lua 5.4 (sem os, io, package, require, load, dofile, debug).
-- Além de string, table, math, utf8, pairs, ipairs, tostring, tonumber, type,
-- select, next, error, pcall e assert, o ambiente oferece:
--
--   finding(message [, severity [, subject]])
--       Registra um finding. severity: "info" | "warning" | "critical"
--       (default "warning"; valor desconhecido também vira "warning").
--       subject (opcional): nome ou id de um nó do trace.
--
--   trace.confidence                     -- 0..100
--   trace.hops, trace.observed, trace.inferred, trace.correlations
--   trace.duration_ms, trace.status, trace.id
--   trace.request.from / .to / .port / .protocol
--
--   flow.rtt_ms                          -- maior RTT do caminho (0 se desconhecido)
--   flow.retransmissions, flow.drops, flow.rst   -- somas ao longo do caminho
--
--   path:contains(x)                     -- x = kind ("nat", "load_balancer", ...) ou
--                                           nome de nó; procura em hops e branches,
--                                           sem distinguir maiúsculas
--   path.hops[i]                         -- { index, entity, name, kind, status, address,
--                                             confidence, rtt_ms, retransmissions, drops,
--                                             rst, edge_status, edge_source }
--   path.branches[i]                     -- { anchor, entity, name, kind, status, address,
--                                             edge_status, direction }
--
--   source / destination                 -- { name, ip, kind, address, entity, port,
--                                             hostname, provider }
--   source:is_private()                  -- IP principal é RFC1918 / CGNAT 100.64/10 /
--                                           ULA / loopback / link-local (false sem IP;
--                                           ::ffff:a.b.c.d segue o v4 embutido)
--
-- Um erro numa regra vira um finding "rule error: ..." (warning) e não impede
-- as outras regras de rodar.

if flow.retransmissions > 5 then
  finding("High TCP retransmission")
end

if flow.rtt_ms > 100 then
  finding("High RTT on path (above 100 ms)")
end

if flow.drops > 0 then
  finding("Kernel drops on path", "critical")
end

if source:is_private() and path:contains("nat") and destination:is_private() then
  finding("Possible traffic tromboning")
end

if trace.confidence < 60 then
  finding("Low path confidence", "info")
end
