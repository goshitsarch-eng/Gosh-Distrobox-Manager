import 'environment_guard_types.dart';
import 'environment_guard_stub.dart'
    if (dart.library.io) 'environment_guard_io.dart' as impl;

EnvironmentGuardResult checkEnvironmentGuard() => impl.checkEnvironmentGuardImpl();
