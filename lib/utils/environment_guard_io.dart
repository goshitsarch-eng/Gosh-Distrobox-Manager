import 'dart:io';
import 'environment_guard_types.dart';

EnvironmentGuardResult checkEnvironmentGuardImpl() {
  final env = Platform.environment;
  final inDistrobox = env.containsKey('DISTROBOX_ENTERED') ||
      env.containsKey('DISTROBOX_CONTAINER_NAME') ||
      env.containsKey('DISTROBOX_HOST_HOME');

  if (!inDistrobox) {
    return const EnvironmentGuardResult(blocked: false);
  }

  final hostExecAvailable = _hasDistroboxHostExec(env);
  if (hostExecAvailable) {
    return const EnvironmentGuardResult(blocked: false);
  }

  return const EnvironmentGuardResult(
    blocked: true,
    message:
        'Detected a Distrobox environment, but `distrobox-host-exec` is not available. '
        'Running commands from inside a Distrobox container can corrupt host Podman/Distrobox storage. '
        'Install `distrobox-host-exec` on the host or run Gosh Distrobox Manager on the host system.',
  );
}

bool _hasDistroboxHostExec(Map<String, String> env) {
  final candidates = <String>[
    '/usr/bin/distrobox-host-exec',
    '/usr/local/bin/distrobox-host-exec',
    '/bin/distrobox-host-exec',
  ];

  final path = env['PATH'] ?? '';
  if (path.isNotEmpty) {
    for (final dir in path.split(':')) {
      if (dir.isEmpty) continue;
      candidates.add('$dir/distrobox-host-exec');
    }
  }

  for (final candidate in candidates) {
    final file = File(candidate);
    if (file.existsSync()) {
      return true;
    }
  }
  return false;
}
