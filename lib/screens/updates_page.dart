import 'package:flutter/material.dart';
import 'package:provider/provider.dart';
import 'package:gosh_distrobox_manager/providers/app_state.dart';
import 'package:gosh_distrobox_manager/src/rust/backends/distrobox/distrobox.dart';

class UpdatesPage extends StatefulWidget {
  const UpdatesPage({super.key});

  @override
  State<UpdatesPage> createState() => _UpdatesPageState();
}

class _UpdatesPageState extends State<UpdatesPage> {
  // Track which containers are currently being upgraded
  final Map<String, String> _upgradingContainers = {}; // name -> taskId

  @override
  void initState() {
    super.initState();
    // Refresh containers on load
    WidgetsBinding.instance.addPostFrameCallback((_) {
      context.read<AppStateProvider>().refresh();
    });
  }

  IconData _getDistroIcon(String image) {
    final imageLower = image.toLowerCase();
    if (imageLower.contains('ubuntu')) return Icons.circle;
    if (imageLower.contains('fedora')) return Icons.filter_vintage;
    if (imageLower.contains('arch')) return Icons.architecture;
    if (imageLower.contains('debian')) return Icons.donut_large;
    if (imageLower.contains('alpine')) return Icons.landscape;
    if (imageLower.contains('centos') || imageLower.contains('rocky')) {
      return Icons.shield;
    }
    if (imageLower.contains('opensuse') || imageLower.contains('suse')) {
      return Icons.pets;
    }
    return Icons.dns;
  }

  Color _getDistroColor(String image) {
    final imageLower = image.toLowerCase();
    if (imageLower.contains('ubuntu')) return Colors.orange;
    if (imageLower.contains('fedora')) return Colors.blue;
    if (imageLower.contains('arch')) return Colors.cyan;
    if (imageLower.contains('debian')) return Colors.red;
    if (imageLower.contains('alpine')) return Colors.blueGrey;
    if (imageLower.contains('centos') || imageLower.contains('rocky')) {
      return Colors.green;
    }
    if (imageLower.contains('opensuse') || imageLower.contains('suse')) {
      return Colors.lightGreen;
    }
    return Colors.purple;
  }

  String _extractImageName(String imageUrl) {
    final parts = imageUrl.split('/');
    final lastPart = parts.last.split(':').first;
    if (lastPart.isEmpty) return 'Container';
    return lastPart[0].toUpperCase() + lastPart.substring(1);
  }

  Future<void> _upgradeContainer(String name) async {
    final appState = context.read<AppStateProvider>();
    final taskId = await appState.upgradeContainer(name);
    if (taskId != null && mounted) {
      setState(() {
        _upgradingContainers[name] = taskId;
      });
      _showUpgradeDialog(name, taskId);
    }
  }

  Future<void> _upgradeAllContainers() async {
    final appState = context.read<AppStateProvider>();
    final runningContainers = appState.containers
        .where((c) => c.status is Status_Up)
        .toList();

    if (runningContainers.isEmpty) {
      ScaffoldMessenger.of(context).showSnackBar(
        const SnackBar(content: Text('No running containers to upgrade')),
      );
      return;
    }

    // Show confirmation dialog
    final confirmed = await showDialog<bool>(
      context: context,
      builder: (context) => AlertDialog(
        title: const Text('Upgrade All Containers'),
        content: Text(
          'This will upgrade packages in ${runningContainers.length} running container(s). Continue?',
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(context, false),
            child: const Text('Cancel'),
          ),
          FilledButton(
            onPressed: () => Navigator.pop(context, true),
            child: const Text('Upgrade All'),
          ),
        ],
      ),
    );

    if (confirmed == true) {
      for (final container in runningContainers) {
        await _upgradeContainer(container.name);
      }
    }
  }

  void _showUpgradeDialog(String containerName, String taskId) {
    showDialog(
      context: context,
      barrierDismissible: false,
      builder: (context) => _UpgradeProgressDialog(
        taskId: taskId,
        containerName: containerName,
        onComplete: () {
          setState(() {
            _upgradingContainers.remove(containerName);
          });
        },
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(
        title: const Row(
          children: [
            Icon(Icons.system_update_alt, color: Color(0xFF137FEC)),
            SizedBox(width: 8),
            Text('Updates', style: TextStyle(fontWeight: FontWeight.bold)),
          ],
        ),
        actions: [
          IconButton(
            icon: const Icon(Icons.refresh),
            tooltip: 'Refresh',
            onPressed: () => context.read<AppStateProvider>().refresh(),
          ),
          Padding(
            padding: const EdgeInsets.only(right: 16.0),
            child: FilledButton.tonal(
              onPressed: _upgradeAllContainers,
              style: FilledButton.styleFrom(
                visualDensity: VisualDensity.compact,
                shape: RoundedRectangleBorder(
                    borderRadius: BorderRadius.circular(20)),
              ),
              child: const Text('Upgrade All',
                  style: TextStyle(fontWeight: FontWeight.bold)),
            ),
          ),
        ],
      ),
      body: Consumer<AppStateProvider>(
        builder: (context, appState, child) {
          if (appState.isLoading) {
            return const Center(child: CircularProgressIndicator());
          }

          if (!appState.isDistroboxInstalled) {
            return Center(
              child: Column(
                mainAxisAlignment: MainAxisAlignment.center,
                children: [
                  const Icon(Icons.warning_amber_rounded,
                      size: 64, color: Colors.orange),
                  const SizedBox(height: 16),
                  const Text('Distrobox Not Found',
                      style:
                          TextStyle(fontSize: 24, fontWeight: FontWeight.bold)),
                  const SizedBox(height: 8),
                  const Text('Please install Distrobox to use this feature.'),
                  const SizedBox(height: 16),
                  FilledButton.tonal(
                    onPressed: () => appState.refresh(),
                    child: const Text('Refresh'),
                  ),
                ],
              ),
            );
          }

          if (appState.containers.isEmpty) {
            return Center(
              child: Column(
                mainAxisAlignment: MainAxisAlignment.center,
                children: [
                  Icon(Icons.inbox_outlined,
                      size: 64, color: Theme.of(context).hintColor),
                  const SizedBox(height: 16),
                  const Text('No Containers',
                      style:
                          TextStyle(fontSize: 20, fontWeight: FontWeight.bold)),
                  const SizedBox(height: 8),
                  Text('Create a container to manage updates.',
                      style: TextStyle(color: Theme.of(context).hintColor)),
                ],
              ),
            );
          }

          final runningContainers =
              appState.containers.where((c) => c.status is Status_Up).toList();
          final stoppedContainers =
              appState.containers.where((c) => c.status is! Status_Up).toList();

          return SingleChildScrollView(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                // Summary Header
                Padding(
                  padding: const EdgeInsets.all(24.0),
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text(
                        '${appState.containers.length} Container${appState.containers.length != 1 ? 's' : ''} Available',
                        style: const TextStyle(
                            fontSize: 24, fontWeight: FontWeight.bold),
                      ),
                      const SizedBox(height: 8),
                      Text(
                        '${runningContainers.length} running, ${stoppedContainers.length} stopped. Upgrade running containers to update their packages.',
                        style: TextStyle(color: Theme.of(context).hintColor),
                      ),
                    ],
                  ),
                ),

                // Running Containers Section
                if (runningContainers.isNotEmpty) ...[
                  Padding(
                    padding: const EdgeInsets.symmetric(horizontal: 16.0),
                    child: Row(
                      children: [
                        Container(
                          width: 8,
                          height: 8,
                          decoration: const BoxDecoration(
                            color: Colors.green,
                            shape: BoxShape.circle,
                          ),
                        ),
                        const SizedBox(width: 8),
                        Text(
                          'RUNNING CONTAINERS',
                          style: TextStyle(
                            fontSize: 12,
                            fontWeight: FontWeight.bold,
                            color: Theme.of(context).hintColor,
                            letterSpacing: 1.0,
                          ),
                        ),
                      ],
                    ),
                  ),
                  const SizedBox(height: 12),
                  ...runningContainers.map((container) => Padding(
                        padding: const EdgeInsets.symmetric(
                            horizontal: 16.0, vertical: 6.0),
                        child: _buildContainerCard(
                          context,
                          container: container,
                          isUpgrading:
                              _upgradingContainers.containsKey(container.name),
                        ),
                      )),
                  const SizedBox(height: 24),
                ],

                // Stopped Containers Section
                if (stoppedContainers.isNotEmpty) ...[
                  Padding(
                    padding: const EdgeInsets.symmetric(horizontal: 16.0),
                    child: Row(
                      children: [
                        Container(
                          width: 8,
                          height: 8,
                          decoration: BoxDecoration(
                            color: Theme.of(context).hintColor,
                            shape: BoxShape.circle,
                          ),
                        ),
                        const SizedBox(width: 8),
                        Text(
                          'STOPPED CONTAINERS',
                          style: TextStyle(
                            fontSize: 12,
                            fontWeight: FontWeight.bold,
                            color: Theme.of(context).hintColor,
                            letterSpacing: 1.0,
                          ),
                        ),
                      ],
                    ),
                  ),
                  const SizedBox(height: 12),
                  ...stoppedContainers.map((container) => Padding(
                        padding: const EdgeInsets.symmetric(
                            horizontal: 16.0, vertical: 6.0),
                        child: Opacity(
                          opacity: 0.7,
                          child: _buildContainerCard(
                            context,
                            container: container,
                            isUpgrading: false,
                            isStopped: true,
                          ),
                        ),
                      )),
                ],

                const SizedBox(height: 80),
              ],
            ),
          );
        },
      ),
    );
  }

  Widget _buildContainerCard(
    BuildContext context, {
    required ContainerInfo container,
    required bool isUpgrading,
    bool isStopped = false,
  }) {
    final distroName = _extractImageName(container.image);
    final distroColor = _getDistroColor(container.image);
    final distroIcon = _getDistroIcon(container.image);

    return Card(
      elevation: 0,
      shape: RoundedRectangleBorder(
        borderRadius: BorderRadius.circular(16),
        side: BorderSide(color: Theme.of(context).dividerColor),
      ),
      child: Padding(
        padding: const EdgeInsets.all(16),
        child: Column(
          children: [
            Row(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Container(
                  width: 48,
                  height: 48,
                  decoration: BoxDecoration(
                    borderRadius: BorderRadius.circular(12),
                    color: distroColor.withOpacity(0.1),
                  ),
                  child: Icon(distroIcon, color: distroColor),
                ),
                const SizedBox(width: 16),
                Expanded(
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Row(
                        mainAxisAlignment: MainAxisAlignment.spaceBetween,
                        children: [
                          Expanded(
                            child: Column(
                              crossAxisAlignment: CrossAxisAlignment.start,
                              children: [
                                Text(container.name,
                                    style: const TextStyle(
                                        fontWeight: FontWeight.bold,
                                        fontSize: 16)),
                                const SizedBox(height: 2),
                                Text(
                                  isStopped
                                      ? 'STOPPED'
                                      : isUpgrading
                                          ? 'UPGRADING...'
                                          : 'READY TO UPGRADE',
                                  style: TextStyle(
                                    color: isStopped
                                        ? Theme.of(context).hintColor
                                        : isUpgrading
                                            ? Colors.blue
                                            : Colors.green,
                                    fontSize: 10,
                                    fontWeight: FontWeight.bold,
                                    letterSpacing: 0.5,
                                  ),
                                ),
                              ],
                            ),
                          ),
                          Container(
                            padding: const EdgeInsets.symmetric(
                                horizontal: 6, vertical: 2),
                            decoration: BoxDecoration(
                              color: Theme.of(context)
                                  .dividerColor
                                  .withOpacity(0.1),
                              borderRadius: BorderRadius.circular(4),
                            ),
                            child: Text(distroName.toUpperCase(),
                                style: TextStyle(
                                    fontSize: 10,
                                    fontWeight: FontWeight.bold,
                                    color: Theme.of(context).hintColor)),
                          ),
                        ],
                      ),
                    ],
                  ),
                ),
              ],
            ),
            const SizedBox(height: 16),
            Row(
              children: [
                Expanded(
                  child: Text(
                    container.image,
                    style: TextStyle(
                        fontSize: 12, color: Theme.of(context).hintColor),
                    overflow: TextOverflow.ellipsis,
                  ),
                ),
              ],
            ),
            if (!isStopped) ...[
              const SizedBox(height: 16),
              Row(
                mainAxisAlignment: MainAxisAlignment.end,
                children: [
                  if (isUpgrading)
                    const SizedBox(
                      width: 20,
                      height: 20,
                      child: CircularProgressIndicator(strokeWidth: 2),
                    )
                  else
                    FilledButton(
                      onPressed: () => _upgradeContainer(container.name),
                      style: FilledButton.styleFrom(
                        visualDensity: VisualDensity.compact,
                        shape: RoundedRectangleBorder(
                            borderRadius: BorderRadius.circular(8)),
                      ),
                      child: const Text('Upgrade'),
                    ),
                ],
              ),
            ] else ...[
              const SizedBox(height: 16),
              Row(
                children: [
                  Icon(Icons.info_outline,
                      size: 16, color: Theme.of(context).hintColor),
                  const SizedBox(width: 8),
                  Text(
                    'Start container to enable upgrades',
                    style: TextStyle(
                        fontSize: 12, color: Theme.of(context).hintColor),
                  ),
                ],
              ),
            ],
          ],
        ),
      ),
    );
  }
}

// Dialog for showing upgrade progress
class _UpgradeProgressDialog extends StatefulWidget {
  final String taskId;
  final String containerName;
  final VoidCallback onComplete;

  const _UpgradeProgressDialog({
    required this.taskId,
    required this.containerName,
    required this.onComplete,
  });

  @override
  State<_UpgradeProgressDialog> createState() => _UpgradeProgressDialogState();
}

class _UpgradeProgressDialogState extends State<_UpgradeProgressDialog> {
  final ScrollController _scrollController = ScrollController();

  @override
  void dispose() {
    _scrollController.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return Consumer<AppStateProvider>(
      builder: (context, appState, child) {
        final task = appState.getTask(widget.taskId);
        final output = task?.output ?? [];
        final isComplete = task?.completed ??
            (output.isNotEmpty &&
                (output.last.contains('completed') ||
                    output.last.contains('failed')));

        // Auto-scroll to bottom
        WidgetsBinding.instance.addPostFrameCallback((_) {
          if (_scrollController.hasClients) {
            _scrollController.animateTo(
              _scrollController.position.maxScrollExtent,
              duration: const Duration(milliseconds: 100),
              curve: Curves.easeOut,
            );
          }
        });

        return AlertDialog(
          title: Text('Upgrading ${widget.containerName}'),
          content: SizedBox(
            width: 400,
            height: 300,
            child: Container(
              padding: const EdgeInsets.all(12),
              decoration: BoxDecoration(
                color: Colors.black87,
                borderRadius: BorderRadius.circular(8),
              ),
              child: output.isEmpty
                  ? const Center(
                      child: Column(
                        mainAxisAlignment: MainAxisAlignment.center,
                        children: [
                          CircularProgressIndicator(color: Colors.green),
                          SizedBox(height: 16),
                          Text('Starting upgrade...',
                              style: TextStyle(color: Colors.green)),
                        ],
                      ),
                    )
                  : ListView.builder(
                      controller: _scrollController,
                      itemCount: output.length,
                      itemBuilder: (context, index) {
                        return Text(
                          output[index],
                          style: const TextStyle(
                            fontFamily: 'monospace',
                            fontSize: 12,
                            color: Colors.green,
                          ),
                        );
                      },
                    ),
            ),
          ),
          actions: [
            if (!isComplete)
              TextButton(
                onPressed: () async {
                  await appState.cancelTask(widget.taskId);
                  widget.onComplete();
                  if (context.mounted) Navigator.pop(context);
                },
                child: const Text('Cancel'),
              ),
            if (isComplete)
              FilledButton(
                onPressed: () {
                  widget.onComplete();
                  Navigator.pop(context);
                },
                child: const Text('Done'),
              ),
          ],
        );
      },
    );
  }
}
