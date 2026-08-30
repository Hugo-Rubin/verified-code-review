# Reserve boundary check.
#
# Prints the current UTC time against the fixed bookkeeping recorded in
# DECISIONS.md. `reserve_trigger_time` is FIXED — it is never recomputed from
# whatever time happens to remain.
#
#     pwsh scripts/deadline.ps1

$deadline = [DateTime]::Parse('2026-08-31T18:00:00Z').ToUniversalTime()
$reserve  = [DateTime]::Parse('2026-08-31T09:39:30Z').ToUniversalTime()
$now      = [DateTime]::UtcNow

function Show-Span([string]$label, [TimeSpan]$span) {
    $sign = if ($span.Ticks -lt 0) { '-' } else { '' }
    $a = $span.Duration()
    '{0,-22} {1}{2}h {3:00}m  ({4}{5:F2} hours)' -f `
        $label, $sign, [math]::Floor($a.TotalHours), $a.Minutes, $sign, $a.TotalHours
}

'now (UTC)              {0}' -f $now.ToString('yyyy-MM-ddTHH:mm:ssZ')
'reserve trigger        2026-08-31T09:39:30Z  (fixed)'
'deadline               2026-08-31T18:00:00Z  (hard)'
''
Show-Span 'until reserve' ($reserve - $now)
Show-Span 'until deadline' ($deadline - $now)
''

if ($now -ge $deadline) {
    'STATUS: DEADLINE PASSED.'
} elseif ($now -ge $reserve) {
    'STATUS: RESERVE REACHED — no new features, experiments, or architecture.'
    '        Reproduction, documentation, results, trajectories, video, submission only.'
} else {
    'STATUS: build phase. Feature and experiment work permitted.'
}
