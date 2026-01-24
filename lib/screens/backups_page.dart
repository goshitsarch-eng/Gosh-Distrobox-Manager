import 'package:flutter/material.dart';
import 'package:provider/provider.dart';
import 'package:gosh_distrobox_manager/providers/app_state.dart';
import 'package:gosh_distrobox_manager/src/rust/backends/distrobox/distrobox.dart';

class BackupsPage extends StatefulWidget {
  const BackupsPage({super.key});

  @override
  State<BackupsPage> createState() => _BackupsPageState();
}

class _BackupsPageState extends State<BackupsPage>
    with SingleTickerProviderStateMixin {
  late TabController _tabController;
  ContainerInfo? _selectedContainer;

  @override
  void initState() {
    super.initState();
    _tabController = TabController(length: 2, vsync: this);
    WidgetsBinding.instance.addPostFrameCallback((_) {
      final appState = context.read<AppStateProvider>();
      appState.refresh();
      appState.loadSnapshots();
      // Select first container if available
      if (appState.containers.isNotEmpty) {
        setState(() {
          _selectedContainer = appState.containers.first;
        });
      }
    });
  }

  @override
  void dispose() {
    _tabController.dispose();
    super.dispose();
  }

  IconData _getDistroIcon(String image) {
    final imageLower = image.toLowerCase();
    if (imageLower.contains('ubuntu')) return Icons.circle;
    if (imageLower.contains('fedora')) return Icons.filter_vintage;
    if (imageLower.contains('arch')) return Icons.architecture;
    if (imageLower.contains('debian')) return Icons.donut_large;
    if (imageLower.contains('alpine')) return Icons.landscape;
    return Icons.dns;
  }

  void _showCreateSnapshotDialog() {
    if (_selectedContainer == null) {
      ScaffoldMessenger.of(context).showSnackBar(
        const SnackBar(content: Text('Please select a container first')),
      );
      return;
    }

    final nameController = TextEditingController(
      text: '${_selectedContainer!.name}-snapshot-${DateTime.now().millisecondsSinceEpoch}',
    );

    showDialog(
      context: context,
      builder: (context) => AlertDialog(
        title: const Text('Create Snapshot'),
        content: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(
              'Create a snapshot of "${_selectedContainer!.name}"',
              style: TextStyle(color: Theme.of(context).hintColor),
            ),
            const SizedBox(height: 16),
            TextField(
              controller: nameController,
              decoration: const InputDecoration(
                labelText: 'Snapshot Name',
                hintText: 'e.g., ubuntu-snapshot-20240101',
                border: OutlineInputBorder(),
                helperText: 'This will create a container image',
              ),
            ),
            const SizedBox(height: 16),
            Container(
              padding: const EdgeInsets.all(12),
              decoration: BoxDecoration(
                color: Colors.blue.withOpacity(0.1),
                borderRadius: BorderRadius.circular(8),
              ),
              child: Row(
                children: [
                  const Icon(Icons.info_outline, color: Colors.blue, size: 20),
                  const SizedBox(width: 8),
                  Expanded(
                    child: Text(
                      'Snapshots are saved as container images using podman/docker commit.',
                      style: TextStyle(fontSize: 12, color: Colors.blue[800]),
                    ),
                  ),
                ],
              ),
            ),
          ],
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(context),
            child: const Text('Cancel'),
          ),
          FilledButton(
            onPressed: () async {
              if (nameController.text.trim().isEmpty) return;
              Navigator.pop(context);

              final appState = context.read<AppStateProvider>();
              final success = await appState.createSnapshot(
                _selectedContainer!.name,
                nameController.text.trim(),
              );

              if (mounted) {
                ScaffoldMessenger.of(context).showSnackBar(
                  SnackBar(
                    content: Text(success
                        ? 'Snapshot created successfully'
                        : 'Failed to create snapshot'),
                    backgroundColor: success ? Colors.green : Colors.red,
                  ),
                );
              }
            },
            child: const Text('Create'),
          ),
        ],
      ),
    );
  }

  void _showDeleteSnapshotDialog(SnapshotInfo snapshot) {
    showDialog(
      context: context,
      builder: (context) => AlertDialog(
        title: const Text('Delete Snapshot'),
        content: Text(
          'Are you sure you want to delete "${snapshot.name}"?\n\n'
          'This action cannot be undone.',
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(context),
            child: const Text('Cancel'),
          ),
          FilledButton(
            onPressed: () async {
              Navigator.pop(context);

              final appState = context.read<AppStateProvider>();
              final success = await appState.deleteSnapshot(snapshot.id);

              if (mounted) {
                ScaffoldMessenger.of(context).showSnackBar(
                  SnackBar(
                    content: Text(success
                        ? 'Snapshot deleted'
                        : 'Failed to delete snapshot'),
                    backgroundColor: success ? Colors.green : Colors.red,
                  ),
                );
              }
            },
            style: FilledButton.styleFrom(backgroundColor: Colors.red),
            child: const Text('Delete'),
          ),
        ],
      ),
    );
  }

  void _showRestoreSnapshotDialog(SnapshotInfo snapshot) {
    final nameController = TextEditingController(
      text: 'restored-${DateTime.now().millisecondsSinceEpoch}',
    );

    showDialog(
      context: context,
      builder: (context) => AlertDialog(
        title: const Text('Restore from Snapshot'),
        content: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(
              'Create a new container from "${snapshot.name}"',
              style: TextStyle(color: Theme.of(context).hintColor),
            ),
            const SizedBox(height: 16),
            TextField(
              controller: nameController,
              decoration: const InputDecoration(
                labelText: 'New Container Name',
                border: OutlineInputBorder(),
              ),
            ),
          ],
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(context),
            child: const Text('Cancel'),
          ),
          FilledButton(
            onPressed: () async {
              if (nameController.text.trim().isEmpty) return;
              Navigator.pop(context);

              final appState = context.read<AppStateProvider>();
              final taskId = await appState.restoreFromSnapshot(
                snapshot.name,
                nameController.text.trim(),
              );

              if (taskId != null && mounted) {
                _showTaskProgressDialog(taskId, 'Restoring ${snapshot.name}');
              }
            },
            child: const Text('Restore'),
          ),
        ],
      ),
    );
  }

  void _showExportDialog() {
    if (_selectedContainer == null) {
      ScaffoldMessenger.of(context).showSnackBar(
        const SnackBar(content: Text('Please select a container first')),
      );
      return;
    }

    final pathController = TextEditingController(
      text: '/tmp/${_selectedContainer!.name}-export.tar',
    );

    showDialog(
      context: context,
      builder: (context) => AlertDialog(
        title: const Text('Export Container'),
        content: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(
              'Export "${_selectedContainer!.name}" as a tar archive',
              style: TextStyle(color: Theme.of(context).hintColor),
            ),
            const SizedBox(height: 16),
            TextField(
              controller: pathController,
              decoration: const InputDecoration(
                labelText: 'Output Path',
                hintText: '/path/to/export.tar',
                border: OutlineInputBorder(),
              ),
            ),
            const SizedBox(height: 16),
            Container(
              padding: const EdgeInsets.all(12),
              decoration: BoxDecoration(
                color: Colors.orange.withOpacity(0.1),
                borderRadius: BorderRadius.circular(8),
              ),
              child: Row(
                children: [
                  const Icon(Icons.warning_amber, color: Colors.orange, size: 20),
                  const SizedBox(width: 8),
                  Expanded(
                    child: Text(
                      'Export may take several minutes depending on container size.',
                      style: TextStyle(fontSize: 12, color: Colors.orange[800]),
                    ),
                  ),
                ],
              ),
            ),
          ],
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(context),
            child: const Text('Cancel'),
          ),
          FilledButton(
            onPressed: () async {
              if (pathController.text.trim().isEmpty) return;
              Navigator.pop(context);

              final appState = context.read<AppStateProvider>();
              final taskId = await appState.exportContainerToFile(
                _selectedContainer!.name,
                pathController.text.trim(),
              );

              if (taskId != null && mounted) {
                _showTaskProgressDialog(
                    taskId, 'Exporting ${_selectedContainer!.name}');
              }
            },
            child: const Text('Export'),
          ),
        ],
      ),
    );
  }

  void _showImportDialog() {
    final pathController = TextEditingController();
    final nameController = TextEditingController();

    showDialog(
      context: context,
      builder: (context) => AlertDialog(
        title: const Text('Import Container'),
        content: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            TextField(
              controller: pathController,
              decoration: const InputDecoration(
                labelText: 'Archive Path',
                hintText: '/path/to/archive.tar',
                border: OutlineInputBorder(),
              ),
            ),
            const SizedBox(height: 16),
            TextField(
              controller: nameController,
              decoration: const InputDecoration(
                labelText: 'Image Name',
                hintText: 'my-imported-image',
                border: OutlineInputBorder(),
                helperText: 'Name for the imported image',
              ),
            ),
          ],
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(context),
            child: const Text('Cancel'),
          ),
          FilledButton(
            onPressed: () async {
              if (pathController.text.trim().isEmpty ||
                  nameController.text.trim().isEmpty) return;
              Navigator.pop(context);

              final appState = context.read<AppStateProvider>();
              final taskId = await appState.importContainerFromFile(
                pathController.text.trim(),
                nameController.text.trim(),
              );

              if (taskId != null && mounted) {
                _showTaskProgressDialog(
                    taskId, 'Importing ${pathController.text}');
              }
            },
            child: const Text('Import'),
          ),
        ],
      ),
    );
  }

  void _showTaskProgressDialog(String taskId, String title) {
    showDialog(
      context: context,
      barrierDismissible: false,
      builder: (context) => _TaskProgressDialog(
        taskId: taskId,
        title: title,
        onComplete: () {
          context.read<AppStateProvider>().loadSnapshots();
          context.read<AppStateProvider>().refresh();
        },
      ),
    );
  }

  void _showCloneDialog() {
    if (_selectedContainer == null) return;

    final nameController = TextEditingController(
      text: '${_selectedContainer!.name}-clone',
    );

    showDialog(
      context: context,
      builder: (context) => AlertDialog(
        title: const Text('Clone Container'),
        content: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(
              'Create a copy of "${_selectedContainer!.name}"',
              style: TextStyle(color: Theme.of(context).hintColor),
            ),
            const SizedBox(height: 16),
            TextField(
              controller: nameController,
              decoration: const InputDecoration(
                labelText: 'New Container Name',
                border: OutlineInputBorder(),
              ),
            ),
          ],
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(context),
            child: const Text('Cancel'),
          ),
          FilledButton(
            onPressed: () async {
              if (nameController.text.isEmpty) return;
              Navigator.pop(context);

              final appState = context.read<AppStateProvider>();
              final args = CreateArgs(
                name: CreateArgName(field0: nameController.text),
                image: _selectedContainer!.image,
                init: false,
                nvidia: false,
                homePath: null,
                volumes: [],
              );

              final taskId = await appState.cloneContainer(
                _selectedContainer!.name,
                args,
              );

              if (taskId != null && mounted) {
                _showTaskProgressDialog(
                    taskId, 'Cloning ${_selectedContainer!.name}');
              }
            },
            child: const Text('Clone'),
          ),
        ],
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(
        title: const Text('Backups & Snapshots'),
        actions: [
          IconButton(
            icon: const Icon(Icons.refresh),
            tooltip: 'Refresh',
            onPressed: () {
              context.read<AppStateProvider>().refresh();
              context.read<AppStateProvider>().loadSnapshots();
            },
          ),
        ],
        bottom: TabBar(
          controller: _tabController,
          tabs: const [
            Tab(text: 'Snapshots', icon: Icon(Icons.camera_alt)),
            Tab(text: 'Export/Import', icon: Icon(Icons.import_export)),
          ],
        ),
      ),
      body: Consumer<AppStateProvider>(
        builder: (context, appState, child) {
          if (appState.isLoading) {
            return const Center(child: CircularProgressIndicator());
          }

          if (!appState.isDistroboxInstalled) {
            return _buildNotInstalledView(context);
          }

          if (appState.containers.isEmpty) {
            return _buildNoContainersView(context);
          }

          // Update selected container if it was removed
          if (_selectedContainer != null &&
              !appState.containers
                  .any((c) => c.name == _selectedContainer!.name)) {
            _selectedContainer = appState.containers.first;
          } else if (_selectedContainer == null) {
            _selectedContainer = appState.containers.first;
          }

          return Column(
            children: [
              // Container Selector
              Padding(
                padding: const EdgeInsets.all(16),
                child: _buildContainerSelector(context, appState),
              ),

              // Tab Content
              Expanded(
                child: TabBarView(
                  controller: _tabController,
                  children: [
                    _buildSnapshotsTab(context, appState),
                    _buildExportImportTab(context),
                  ],
                ),
              ),
            ],
          );
        },
      ),
      floatingActionButton: FloatingActionButton.extended(
        onPressed: _showCreateSnapshotDialog,
        icon: const Icon(Icons.add_a_photo),
        label: const Text('New Snapshot'),
      ),
    );
  }

  Widget _buildContainerSelector(
      BuildContext context, AppStateProvider appState) {
    return Card(
      elevation: 0,
      shape: RoundedRectangleBorder(
        borderRadius: BorderRadius.circular(12),
        side: BorderSide(color: Theme.of(context).dividerColor),
      ),
      child: Padding(
        padding: const EdgeInsets.all(12),
        child: Row(
          children: [
            Container(
              width: 40,
              height: 40,
              decoration: BoxDecoration(
                color: Theme.of(context).colorScheme.primary.withOpacity(0.1),
                borderRadius: BorderRadius.circular(10),
              ),
              child: Icon(
                _selectedContainer != null
                    ? _getDistroIcon(_selectedContainer!.image)
                    : Icons.dns,
                color: Theme.of(context).colorScheme.primary,
              ),
            ),
            const SizedBox(width: 12),
            Expanded(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(
                    'SELECTED CONTAINER',
                    style: TextStyle(
                      fontSize: 10,
                      fontWeight: FontWeight.bold,
                      color: Theme.of(context).hintColor,
                      letterSpacing: 0.5,
                    ),
                  ),
                  const SizedBox(height: 2),
                  DropdownButton<String>(
                    value: _selectedContainer?.name,
                    icon: const Icon(Icons.expand_more),
                    underline: const SizedBox(),
                    isDense: true,
                    style: TextStyle(
                      fontSize: 16,
                      fontWeight: FontWeight.bold,
                      color: Theme.of(context).textTheme.bodyLarge?.color,
                    ),
                    onChanged: (String? newValue) {
                      if (newValue != null) {
                        setState(() {
                          _selectedContainer = appState.containers
                              .firstWhere((c) => c.name == newValue);
                        });
                      }
                    },
                    items: appState.containers.map((container) {
                      return DropdownMenuItem<String>(
                        value: container.name,
                        child: Text(container.name),
                      );
                    }).toList(),
                  ),
                ],
              ),
            ),
          ],
        ),
      ),
    );
  }

  Widget _buildSnapshotsTab(BuildContext context, AppStateProvider appState) {
    if (appState.isLoadingSnapshots) {
      return const Center(
        child: Column(
          mainAxisAlignment: MainAxisAlignment.center,
          children: [
            CircularProgressIndicator(),
            SizedBox(height: 16),
            Text('Loading snapshots...'),
          ],
        ),
      );
    }

    if (appState.snapshotsError != null) {
      return Center(
        child: Column(
          mainAxisAlignment: MainAxisAlignment.center,
          children: [
            const Icon(Icons.error_outline, size: 64, color: Colors.red),
            const SizedBox(height: 16),
            Text(appState.snapshotsError!),
            const SizedBox(height: 16),
            ElevatedButton(
              onPressed: () => appState.loadSnapshots(),
              child: const Text('Retry'),
            ),
          ],
        ),
      );
    }

    final snapshots = appState.snapshots;

    if (snapshots.isEmpty) {
      return Center(
        child: Padding(
          padding: const EdgeInsets.all(32),
          child: Column(
            mainAxisAlignment: MainAxisAlignment.center,
            children: [
              Container(
                padding: const EdgeInsets.all(24),
                decoration: BoxDecoration(
                  color: Theme.of(context).colorScheme.primary.withOpacity(0.1),
                  shape: BoxShape.circle,
                ),
                child: Icon(
                  Icons.camera_alt_outlined,
                  size: 64,
                  color: Theme.of(context).colorScheme.primary,
                ),
              ),
              const SizedBox(height: 24),
              const Text(
                'No Snapshots Yet',
                style: TextStyle(fontSize: 20, fontWeight: FontWeight.bold),
              ),
              const SizedBox(height: 8),
              Text(
                'Create a snapshot to save the current state of your container.',
                textAlign: TextAlign.center,
                style: TextStyle(color: Theme.of(context).hintColor),
              ),
              const SizedBox(height: 24),
              FilledButton.icon(
                onPressed: _showCreateSnapshotDialog,
                icon: const Icon(Icons.add_a_photo),
                label: const Text('Create First Snapshot'),
              ),
            ],
          ),
        ),
      );
    }

    return ListView.builder(
      padding: const EdgeInsets.symmetric(horizontal: 16),
      itemCount: snapshots.length,
      itemBuilder: (context, index) {
        final snapshot = snapshots[index];
        return _buildSnapshotCard(context, snapshot);
      },
    );
  }

  Widget _buildSnapshotCard(BuildContext context, SnapshotInfo snapshot) {
    return Card(
      margin: const EdgeInsets.only(bottom: 12),
      elevation: 0,
      shape: RoundedRectangleBorder(
        borderRadius: BorderRadius.circular(12),
        side: BorderSide(color: Theme.of(context).dividerColor),
      ),
      child: Padding(
        padding: const EdgeInsets.all(16),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Row(
              children: [
                Container(
                  padding: const EdgeInsets.all(10),
                  decoration: BoxDecoration(
                    color: Theme.of(context).colorScheme.primary.withOpacity(0.1),
                    borderRadius: BorderRadius.circular(10),
                  ),
                  child: Icon(
                    Icons.camera,
                    color: Theme.of(context).colorScheme.primary,
                  ),
                ),
                const SizedBox(width: 12),
                Expanded(
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text(
                        snapshot.name,
                        style: const TextStyle(
                          fontWeight: FontWeight.bold,
                          fontSize: 16,
                        ),
                        overflow: TextOverflow.ellipsis,
                      ),
                      const SizedBox(height: 4),
                      Row(
                        children: [
                          Icon(Icons.schedule,
                              size: 14, color: Theme.of(context).hintColor),
                          const SizedBox(width: 4),
                          Text(
                            snapshot.created,
                            style: TextStyle(
                              fontSize: 12,
                              color: Theme.of(context).hintColor,
                            ),
                          ),
                          const SizedBox(width: 12),
                          Icon(Icons.storage,
                              size: 14, color: Theme.of(context).hintColor),
                          const SizedBox(width: 4),
                          Text(
                            snapshot.size,
                            style: TextStyle(
                              fontSize: 12,
                              color: Theme.of(context).hintColor,
                            ),
                          ),
                        ],
                      ),
                    ],
                  ),
                ),
              ],
            ),
            const SizedBox(height: 12),
            Row(
              mainAxisAlignment: MainAxisAlignment.end,
              children: [
                TextButton.icon(
                  onPressed: () => _showRestoreSnapshotDialog(snapshot),
                  icon: const Icon(Icons.restore, size: 18),
                  label: const Text('Restore'),
                ),
                const SizedBox(width: 8),
                TextButton.icon(
                  onPressed: () => _showDeleteSnapshotDialog(snapshot),
                  icon: const Icon(Icons.delete_outline,
                      size: 18, color: Colors.red),
                  label: const Text('Delete',
                      style: TextStyle(color: Colors.red)),
                ),
              ],
            ),
          ],
        ),
      ),
    );
  }

  Widget _buildExportImportTab(BuildContext context) {
    return SingleChildScrollView(
      padding: const EdgeInsets.all(16),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          // Export Section
          Text(
            'EXPORT',
            style: TextStyle(
              fontSize: 12,
              fontWeight: FontWeight.bold,
              color: Theme.of(context).hintColor,
              letterSpacing: 1.0,
            ),
          ),
          const SizedBox(height: 12),
          _buildActionCard(
            context,
            icon: Icons.upload_file,
            title: 'Export Container',
            description:
                'Export the selected container as a tar archive that can be imported later or on another system.',
            buttonLabel: 'Export',
            buttonIcon: Icons.save_alt,
            onPressed: _showExportDialog,
          ),
          const SizedBox(height: 24),

          // Import Section
          Text(
            'IMPORT',
            style: TextStyle(
              fontSize: 12,
              fontWeight: FontWeight.bold,
              color: Theme.of(context).hintColor,
              letterSpacing: 1.0,
            ),
          ),
          const SizedBox(height: 12),
          _buildActionCard(
            context,
            icon: Icons.download,
            title: 'Import Container',
            description:
                'Import a previously exported container archive to create a new image.',
            buttonLabel: 'Import',
            buttonIcon: Icons.file_open,
            onPressed: _showImportDialog,
          ),
          const SizedBox(height: 24),

          // Clone Section
          Text(
            'CLONE',
            style: TextStyle(
              fontSize: 12,
              fontWeight: FontWeight.bold,
              color: Theme.of(context).hintColor,
              letterSpacing: 1.0,
            ),
          ),
          const SizedBox(height: 12),
          _buildActionCard(
            context,
            icon: Icons.content_copy,
            title: 'Clone Container',
            description:
                'Create an exact copy of the selected container with a new name using distrobox clone.',
            buttonLabel: 'Clone',
            buttonIcon: Icons.copy_all,
            onPressed: _selectedContainer != null ? _showCloneDialog : null,
          ),

          const SizedBox(height: 80),
        ],
      ),
    );
  }

  Widget _buildActionCard(
    BuildContext context, {
    required IconData icon,
    required String title,
    required String description,
    required String buttonLabel,
    required IconData buttonIcon,
    required VoidCallback? onPressed,
  }) {
    return Card(
      elevation: 0,
      shape: RoundedRectangleBorder(
        borderRadius: BorderRadius.circular(16),
        side: BorderSide(color: Theme.of(context).dividerColor),
      ),
      child: Padding(
        padding: const EdgeInsets.all(16),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Row(
              children: [
                Container(
                  padding: const EdgeInsets.all(10),
                  decoration: BoxDecoration(
                    color:
                        Theme.of(context).colorScheme.primary.withOpacity(0.1),
                    borderRadius: BorderRadius.circular(10),
                  ),
                  child:
                      Icon(icon, color: Theme.of(context).colorScheme.primary),
                ),
                const SizedBox(width: 12),
                Expanded(
                  child: Text(
                    title,
                    style: const TextStyle(
                      fontWeight: FontWeight.bold,
                      fontSize: 16,
                    ),
                  ),
                ),
              ],
            ),
            const SizedBox(height: 12),
            Text(
              description,
              style: TextStyle(
                color: Theme.of(context).hintColor,
                fontSize: 14,
              ),
            ),
            const SizedBox(height: 16),
            SizedBox(
              width: double.infinity,
              child: FilledButton.icon(
                onPressed: onPressed,
                icon: Icon(buttonIcon),
                label: Text(buttonLabel),
              ),
            ),
          ],
        ),
      ),
    );
  }

  Widget _buildNotInstalledView(BuildContext context) {
    return Center(
      child: Column(
        mainAxisAlignment: MainAxisAlignment.center,
        children: [
          const Icon(Icons.warning_amber_rounded,
              size: 64, color: Colors.orange),
          const SizedBox(height: 16),
          const Text('Distrobox Not Found',
              style: TextStyle(fontSize: 24, fontWeight: FontWeight.bold)),
          const SizedBox(height: 8),
          const Text('Please install Distrobox to use this feature.'),
        ],
      ),
    );
  }

  Widget _buildNoContainersView(BuildContext context) {
    return Center(
      child: Column(
        mainAxisAlignment: MainAxisAlignment.center,
        children: [
          Icon(Icons.backup_outlined,
              size: 64, color: Theme.of(context).hintColor),
          const SizedBox(height: 16),
          const Text('No Containers',
              style: TextStyle(fontSize: 20, fontWeight: FontWeight.bold)),
          const SizedBox(height: 8),
          Text('Create a container to manage backups.',
              style: TextStyle(color: Theme.of(context).hintColor)),
        ],
      ),
    );
  }
}

/// Dialog showing task progress with streaming output
class _TaskProgressDialog extends StatefulWidget {
  final String taskId;
  final String title;
  final VoidCallback? onComplete;

  const _TaskProgressDialog({
    required this.taskId,
    required this.title,
    this.onComplete,
  });

  @override
  State<_TaskProgressDialog> createState() => _TaskProgressDialogState();
}

class _TaskProgressDialogState extends State<_TaskProgressDialog> {
  final ScrollController _scrollController = ScrollController();
  bool _isComplete = false;

  @override
  void initState() {
    super.initState();
    _checkCompletion();
  }

  void _checkCompletion() async {
    while (mounted && !_isComplete) {
      await Future.delayed(const Duration(milliseconds: 500));
      if (!mounted) return;

      final appState = context.read<AppStateProvider>();
      final isRunning = await appState.isTaskRunning(widget.taskId);

      if (!isRunning) {
        setState(() => _isComplete = true);
        widget.onComplete?.call();
      }
    }
  }

  @override
  void dispose() {
    _scrollController.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return AlertDialog(
      title: Row(
        children: [
          if (!_isComplete) ...[
            const SizedBox(
              width: 20,
              height: 20,
              child: CircularProgressIndicator(strokeWidth: 2),
            ),
            const SizedBox(width: 12),
          ] else ...[
            const Icon(Icons.check_circle, color: Colors.green, size: 24),
            const SizedBox(width: 12),
          ],
          Expanded(child: Text(widget.title)),
        ],
      ),
      content: SizedBox(
        width: double.maxFinite,
        height: 300,
        child: Consumer<AppStateProvider>(
          builder: (context, appState, child) {
            final task = appState.getTask(widget.taskId);
            final output = task?.output ?? [];

            WidgetsBinding.instance.addPostFrameCallback((_) {
              if (_scrollController.hasClients) {
                _scrollController.animateTo(
                  _scrollController.position.maxScrollExtent,
                  duration: const Duration(milliseconds: 100),
                  curve: Curves.easeOut,
                );
              }
            });

            return Container(
              padding: const EdgeInsets.all(12),
              decoration: BoxDecoration(
                color: Colors.black87,
                borderRadius: BorderRadius.circular(8),
              ),
              child: ListView.builder(
                controller: _scrollController,
                itemCount: output.length,
                itemBuilder: (context, index) {
                  return Text(
                    output[index],
                    style: const TextStyle(
                      fontFamily: 'monospace',
                      fontSize: 11,
                      color: Colors.white,
                    ),
                  );
                },
              ),
            );
          },
        ),
      ),
      actions: [
        if (_isComplete)
          FilledButton(
            onPressed: () => Navigator.pop(context),
            child: const Text('Done'),
          )
        else
          TextButton(
            onPressed: () {
              context.read<AppStateProvider>().cancelTask(widget.taskId);
              Navigator.pop(context);
            },
            child: const Text('Cancel'),
          ),
      ],
    );
  }
}
