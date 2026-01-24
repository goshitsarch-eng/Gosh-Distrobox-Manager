import 'dart:async';
import 'package:flutter/material.dart';
import 'package:gosh_distrobox_manager/src/rust/api.dart' as api;
import 'package:gosh_distrobox_manager/src/rust/backends/distrobox/distrobox.dart';

/// Represents a running task in the application
class TaskInfo {
  final String id;
  final String description;
  final List<String> output;
  final DateTime startTime;
  StreamSubscription<String>? _subscription;

  TaskInfo({
    required this.id,
    required this.description,
    List<String>? output,
  })  : output = output ?? [],
        startTime = DateTime.now();

  void addOutput(String line) {
    output.add(line);
  }

  void cancel() {
    _subscription?.cancel();
  }
}

class AppStateProvider extends ChangeNotifier {
  // Container state
  List<ContainerInfo> _containers = [];
  bool _isLoading = false;
  String? _error;
  bool _isDistroboxInstalled = true;
  ContainerInfo? _selectedContainer;

  // Apps state (for the selected container)
  List<api.AppInfo> _containerApps = [];
  bool _isLoadingApps = false;
  String? _appsError;

  // Images state
  List<String> _availableImages = [];
  bool _isLoadingImages = false;
  String? _imagesError;

  // Exported binaries state
  List<api.ExportedBinary> _exportedBinaries = [];
  bool _isLoadingBinaries = false;
  String? _binariesError;

  // Package management state
  List<PackageInfo> _installedPackages = [];
  List<PackageInfo> _searchResults = [];
  String? _detectedPackageManager;
  bool _isLoadingPackages = false;
  String? _packagesError;

  // Snapshot/backup state
  List<SnapshotInfo> _snapshots = [];
  bool _isLoadingSnapshots = false;
  String? _snapshotsError;

  // Container stats state
  ContainerStats? _containerStats;
  bool _isLoadingStats = false;

  // Tasks state
  final Map<String, TaskInfo> _activeTasks = {};
  bool _isActionInProgress = false;
  String? _actionError;

  // Getters for container state
  List<ContainerInfo> get containers => _containers;
  bool get isLoading => _isLoading;
  String? get error => _error;
  bool get isDistroboxInstalled => _isDistroboxInstalled;
  ContainerInfo? get selectedContainer => _selectedContainer;

  // Getters for apps state
  List<api.AppInfo> get containerApps => _containerApps;
  bool get isLoadingApps => _isLoadingApps;
  String? get appsError => _appsError;

  // Getters for images state
  List<String> get availableImages => _availableImages;
  bool get isLoadingImages => _isLoadingImages;
  String? get imagesError => _imagesError;

  // Getters for binaries state
  List<api.ExportedBinary> get exportedBinaries => _exportedBinaries;
  bool get isLoadingBinaries => _isLoadingBinaries;
  String? get binariesError => _binariesError;

  // Getters for packages state
  List<PackageInfo> get installedPackages => _installedPackages;
  List<PackageInfo> get searchResults => _searchResults;
  String? get detectedPackageManager => _detectedPackageManager;
  bool get isLoadingPackages => _isLoadingPackages;
  String? get packagesError => _packagesError;

  // Getters for snapshots state
  List<SnapshotInfo> get snapshots => _snapshots;
  bool get isLoadingSnapshots => _isLoadingSnapshots;
  String? get snapshotsError => _snapshotsError;

  // Getters for container stats state
  ContainerStats? get containerStats => _containerStats;
  bool get isLoadingStats => _isLoadingStats;

  // Getters for tasks state
  Map<String, TaskInfo> get activeTasks => _activeTasks;
  bool get isActionInProgress => _isActionInProgress;
  String? get actionError => _actionError;

  // Helper getters
  int get runningContainersCount =>
      _containers.where((c) => c.status is Status_Up).length;
  int get stoppedContainersCount =>
      _containers.where((c) => c.status is! Status_Up).length;

  AppStateProvider() {
    refresh();
  }

  // ============================================================================
  // Container Management
  // ============================================================================

  Future<void> refresh() async {
    _isLoading = true;
    notifyListeners();
    try {
      _isDistroboxInstalled = await api.isDistroboxInstalled();
      if (_isDistroboxInstalled) {
        _containers = await api.getContainers();
        _error = null;
      } else {
        _containers = [];
      }
    } catch (e) {
      _error = e.toString();
    } finally {
      _isLoading = false;
      notifyListeners();
    }
  }

  void selectContainer(ContainerInfo? container) {
    _selectedContainer = container;
    // Clear previous container-specific data
    _containerApps = [];
    _exportedBinaries = [];
    _appsError = null;
    _binariesError = null;
    notifyListeners();
  }

  Future<bool> removeContainer(String name) async {
    _isActionInProgress = true;
    _actionError = null;
    notifyListeners();

    try {
      await api.removeContainer(name: name);
      // Refresh container list after successful removal
      await refresh();
      return true;
    } catch (e) {
      _actionError = 'Failed to remove container: $e';
      notifyListeners();
      return false;
    } finally {
      _isActionInProgress = false;
      notifyListeners();
    }
  }

  Future<bool> stopContainer(String name) async {
    _isActionInProgress = true;
    _actionError = null;
    notifyListeners();

    try {
      await api.stopContainer(name: name);
      // Refresh to update status
      await refresh();
      return true;
    } catch (e) {
      _actionError = 'Failed to stop container: $e';
      notifyListeners();
      return false;
    } finally {
      _isActionInProgress = false;
      notifyListeners();
    }
  }

  Future<bool> stopAllContainers() async {
    _isActionInProgress = true;
    _actionError = null;
    notifyListeners();

    try {
      await api.stopAllContainers();
      await refresh();
      return true;
    } catch (e) {
      _actionError = 'Failed to stop all containers: $e';
      notifyListeners();
      return false;
    } finally {
      _isActionInProgress = false;
      notifyListeners();
    }
  }

  /// Upgrade a container (returns task ID for tracking)
  Future<String?> upgradeContainer(String name) async {
    _isActionInProgress = true;
    _actionError = null;
    notifyListeners();

    try {
      final taskId = await api.upgradeContainer(name: name);
      _startTaskTracking(taskId, 'Upgrading $name');
      return taskId;
    } catch (e) {
      _actionError = 'Failed to start upgrade: $e';
      notifyListeners();
      return null;
    } finally {
      _isActionInProgress = false;
      notifyListeners();
    }
  }

  /// Clone a container (returns task ID for tracking)
  Future<String?> cloneContainer(String sourceName, CreateArgs args) async {
    _isActionInProgress = true;
    _actionError = null;
    notifyListeners();

    try {
      final taskId =
          await api.cloneContainer(sourceName: sourceName, args: args);
      _startTaskTracking(taskId, 'Cloning $sourceName to ${args.name}');
      return taskId;
    } catch (e) {
      _actionError = 'Failed to start clone: $e';
      notifyListeners();
      return null;
    } finally {
      _isActionInProgress = false;
      notifyListeners();
    }
  }

  /// Create a new container (returns task ID for tracking)
  Future<String?> createContainer(CreateArgs args) async {
    _isActionInProgress = true;
    _actionError = null;
    notifyListeners();

    try {
      final taskId = await api.createContainer(args: args);
      _startTaskTracking(taskId, 'Creating ${args.name}');
      return taskId;
    } catch (e) {
      _actionError = 'Failed to start container creation: $e';
      notifyListeners();
      return null;
    } finally {
      _isActionInProgress = false;
      notifyListeners();
    }
  }

  /// Get the command to enter a container shell
  Future<List<String>> getEnterCommand(String name) async {
    return await api.getEnterCommand(name: name);
  }

  // ============================================================================
  // Application Export Management
  // ============================================================================

  Future<void> loadContainerApps(String containerName) async {
    _isLoadingApps = true;
    _appsError = null;
    notifyListeners();

    try {
      _containerApps =
          await api.listContainerApps(containerName: containerName);
    } catch (e) {
      _appsError = 'Failed to load apps: $e';
    } finally {
      _isLoadingApps = false;
      notifyListeners();
    }
  }

  Future<bool> exportApp(String containerName, String desktopFilePath) async {
    _isActionInProgress = true;
    _actionError = null;
    notifyListeners();

    try {
      await api.exportApp(
          containerName: containerName, desktopFilePath: desktopFilePath);
      // Refresh apps list to update export status
      await loadContainerApps(containerName);
      return true;
    } catch (e) {
      _actionError = 'Failed to export app: $e';
      notifyListeners();
      return false;
    } finally {
      _isActionInProgress = false;
      notifyListeners();
    }
  }

  Future<bool> unexportApp(
      String containerName, String desktopFilePath) async {
    _isActionInProgress = true;
    _actionError = null;
    notifyListeners();

    try {
      await api.unexportApp(
          containerName: containerName, desktopFilePath: desktopFilePath);
      // Refresh apps list to update export status
      await loadContainerApps(containerName);
      return true;
    } catch (e) {
      _actionError = 'Failed to unexport app: $e';
      notifyListeners();
      return false;
    } finally {
      _isActionInProgress = false;
      notifyListeners();
    }
  }

  /// Toggle app export status
  Future<bool> toggleAppExport(
      String containerName, api.AppInfo app, bool export) async {
    if (export) {
      return await exportApp(containerName, app.desktopFilePath);
    } else {
      return await unexportApp(containerName, app.desktopFilePath);
    }
  }

  // ============================================================================
  // Binary Export Management
  // ============================================================================

  Future<void> loadExportedBinaries(String containerName) async {
    _isLoadingBinaries = true;
    _binariesError = null;
    notifyListeners();

    try {
      _exportedBinaries =
          await api.listExportedBinaries(containerName: containerName);
    } catch (e) {
      _binariesError = 'Failed to load exported binaries: $e';
    } finally {
      _isLoadingBinaries = false;
      notifyListeners();
    }
  }

  Future<bool> exportBinary(String containerName, String binaryPath) async {
    _isActionInProgress = true;
    _actionError = null;
    notifyListeners();

    try {
      await api.exportBinary(
          containerName: containerName, binaryPath: binaryPath);
      // Refresh binaries list
      await loadExportedBinaries(containerName);
      return true;
    } catch (e) {
      _actionError = 'Failed to export binary: $e';
      notifyListeners();
      return false;
    } finally {
      _isActionInProgress = false;
      notifyListeners();
    }
  }

  Future<bool> unexportBinary(String containerName, String binaryPath) async {
    _isActionInProgress = true;
    _actionError = null;
    notifyListeners();

    try {
      await api.unexportBinary(
          containerName: containerName, binaryPath: binaryPath);
      // Refresh binaries list
      await loadExportedBinaries(containerName);
      return true;
    } catch (e) {
      _actionError = 'Failed to unexport binary: $e';
      notifyListeners();
      return false;
    } finally {
      _isActionInProgress = false;
      notifyListeners();
    }
  }

  // ============================================================================
  // Image Management
  // ============================================================================

  Future<void> loadAvailableImages() async {
    _isLoadingImages = true;
    _imagesError = null;
    notifyListeners();

    try {
      _availableImages = await api.listAvailableImages();
    } catch (e) {
      _imagesError = 'Failed to load available images: $e';
    } finally {
      _isLoadingImages = false;
      notifyListeners();
    }
  }

  // ============================================================================
  // Task Management
  // ============================================================================

  void _startTaskTracking(String taskId, String description) {
    final task = TaskInfo(id: taskId, description: description);

    // Subscribe to task output
    task._subscription = api.streamTaskOutput(taskId: taskId).listen(
      (output) {
        task.addOutput(output);
        notifyListeners();
      },
      onDone: () {
        // Task completed - refresh containers
        refresh();
      },
      onError: (error) {
        task.addOutput('Error: $error');
        notifyListeners();
      },
    );

    _activeTasks[taskId] = task;
    notifyListeners();
  }

  TaskInfo? getTask(String taskId) => _activeTasks[taskId];

  Future<bool> isTaskRunning(String taskId) async {
    return await api.isTaskRunning(taskId: taskId);
  }

  Future<bool> cancelTask(String taskId) async {
    final task = _activeTasks[taskId];
    if (task != null) {
      task.cancel();
      final result = await api.cancelTask(taskId: taskId);
      if (result) {
        _activeTasks.remove(taskId);
        notifyListeners();
      }
      return result;
    }
    return false;
  }

  void clearCompletedTasks() {
    _activeTasks.removeWhere((id, task) => task._subscription == null);
    notifyListeners();
  }

  void clearActionError() {
    _actionError = null;
    notifyListeners();
  }

  // ============================================================================
  // Package Management
  // ============================================================================

  /// Detect the package manager used in a container
  Future<String?> detectPackageManager(String containerName) async {
    try {
      _detectedPackageManager = await api.detectPackageManager(containerName: containerName);
      notifyListeners();
      return _detectedPackageManager;
    } catch (e) {
      _packagesError = 'Failed to detect package manager: $e';
      notifyListeners();
      return null;
    }
  }

  /// Load installed packages for a container
  Future<void> loadInstalledPackages(String containerName) async {
    _isLoadingPackages = true;
    _packagesError = null;
    notifyListeners();

    try {
      _installedPackages = await api.listInstalledPackages(containerName: containerName);
    } catch (e) {
      _packagesError = 'Failed to load packages: $e';
    } finally {
      _isLoadingPackages = false;
      notifyListeners();
    }
  }

  /// Search for packages in a container
  Future<void> searchPackages(String containerName, String query) async {
    if (query.trim().isEmpty) {
      _searchResults = [];
      notifyListeners();
      return;
    }

    _isLoadingPackages = true;
    _packagesError = null;
    notifyListeners();

    try {
      _searchResults = await api.searchPackages(containerName: containerName, query: query);
    } catch (e) {
      _packagesError = 'Failed to search packages: $e';
    } finally {
      _isLoadingPackages = false;
      notifyListeners();
    }
  }

  /// Clear package search results
  void clearSearchResults() {
    _searchResults = [];
    notifyListeners();
  }

  /// Install a package in a container (returns task ID)
  Future<String?> installPackage(String containerName, String packageName) async {
    _isActionInProgress = true;
    _actionError = null;
    notifyListeners();

    try {
      final taskId = await api.installPackage(containerName: containerName, packageName: packageName);
      _startTaskTracking(taskId, 'Installing $packageName in $containerName');
      return taskId;
    } catch (e) {
      _actionError = 'Failed to install package: $e';
      notifyListeners();
      return null;
    } finally {
      _isActionInProgress = false;
      notifyListeners();
    }
  }

  /// Remove a package from a container (returns task ID)
  Future<String?> removePackage(String containerName, String packageName) async {
    _isActionInProgress = true;
    _actionError = null;
    notifyListeners();

    try {
      final taskId = await api.removePackage(containerName: containerName, packageName: packageName);
      _startTaskTracking(taskId, 'Removing $packageName from $containerName');
      return taskId;
    } catch (e) {
      _actionError = 'Failed to remove package: $e';
      notifyListeners();
      return null;
    } finally {
      _isActionInProgress = false;
      notifyListeners();
    }
  }

  // ============================================================================
  // Snapshot/Backup Management
  // ============================================================================

  /// Load available snapshots
  Future<void> loadSnapshots({String? filterPrefix}) async {
    _isLoadingSnapshots = true;
    _snapshotsError = null;
    notifyListeners();

    try {
      _snapshots = await api.listSnapshots(filterPrefix: filterPrefix);
    } catch (e) {
      _snapshotsError = 'Failed to load snapshots: $e';
    } finally {
      _isLoadingSnapshots = false;
      notifyListeners();
    }
  }

  /// Create a snapshot of a container
  Future<bool> createSnapshot(String containerName, String snapshotName) async {
    _isActionInProgress = true;
    _actionError = null;
    notifyListeners();

    try {
      await api.createSnapshot(containerName: containerName, snapshotName: snapshotName);
      // Refresh snapshots list
      await loadSnapshots();
      return true;
    } catch (e) {
      _actionError = 'Failed to create snapshot: $e';
      notifyListeners();
      return false;
    } finally {
      _isActionInProgress = false;
      notifyListeners();
    }
  }

  /// Delete a snapshot
  Future<bool> deleteSnapshot(String snapshotNameOrId) async {
    _isActionInProgress = true;
    _actionError = null;
    notifyListeners();

    try {
      await api.deleteSnapshot(snapshotNameOrId: snapshotNameOrId);
      // Refresh snapshots list
      await loadSnapshots();
      return true;
    } catch (e) {
      _actionError = 'Failed to delete snapshot: $e';
      notifyListeners();
      return false;
    } finally {
      _isActionInProgress = false;
      notifyListeners();
    }
  }

  /// Restore from a snapshot (creates a new container, returns task ID)
  Future<String?> restoreFromSnapshot(String snapshotName, String newContainerName) async {
    _isActionInProgress = true;
    _actionError = null;
    notifyListeners();

    try {
      final taskId = await api.restoreFromSnapshot(
        snapshotName: snapshotName, 
        newContainerName: newContainerName
      );
      _startTaskTracking(taskId, 'Restoring $snapshotName to $newContainerName');
      return taskId;
    } catch (e) {
      _actionError = 'Failed to restore snapshot: $e';
      notifyListeners();
      return null;
    } finally {
      _isActionInProgress = false;
      notifyListeners();
    }
  }

  /// Export a container to a file (returns task ID)
  Future<String?> exportContainerToFile(String containerName, String outputPath) async {
    _isActionInProgress = true;
    _actionError = null;
    notifyListeners();

    try {
      final taskId = await api.exportContainerToFile(
        containerName: containerName, 
        outputPath: outputPath
      );
      _startTaskTracking(taskId, 'Exporting $containerName to $outputPath');
      return taskId;
    } catch (e) {
      _actionError = 'Failed to export container: $e';
      notifyListeners();
      return null;
    } finally {
      _isActionInProgress = false;
      notifyListeners();
    }
  }

  /// Import a container from a file (returns task ID)
  Future<String?> importContainerFromFile(String archivePath, String imageName) async {
    _isActionInProgress = true;
    _actionError = null;
    notifyListeners();

    try {
      final taskId = await api.importContainerFromFile(
        archivePath: archivePath, 
        imageName: imageName
      );
      _startTaskTracking(taskId, 'Importing $archivePath as $imageName');
      return taskId;
    } catch (e) {
      _actionError = 'Failed to import container: $e';
      notifyListeners();
      return null;
    } finally {
      _isActionInProgress = false;
      notifyListeners();
    }
  }

  // ============================================================================
  // Container Stats
  // ============================================================================

  /// Get resource stats for a container
  Future<void> loadContainerStats(String containerName) async {
    _isLoadingStats = true;
    notifyListeners();

    try {
      _containerStats = await api.getContainerStats(containerName: containerName);
    } catch (e) {
      // Stats might fail for stopped containers, which is expected
      _containerStats = null;
    } finally {
      _isLoadingStats = false;
      notifyListeners();
    }
  }

  /// Run an arbitrary command in a container
  Future<String?> runCommandInContainer(String containerName, String command) async {
    try {
      return await api.runCommandInContainer(containerName: containerName, command: command);
    } catch (e) {
      _actionError = 'Failed to run command: $e';
      notifyListeners();
      return null;
    }
  }

  @override
  void dispose() {
    // Cancel all task subscriptions
    for (final task in _activeTasks.values) {
      task.cancel();
    }
    super.dispose();
  }
}
