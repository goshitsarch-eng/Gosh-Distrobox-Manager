import 'package:flutter/material.dart';
import 'package:provider/provider.dart';
import 'package:gosh_distrobox_manager/providers/app_state.dart';
import 'package:gosh_distrobox_manager/src/rust/backends/distrobox/distrobox.dart';
import 'package:gosh_distrobox_manager/screens/container_details_page.dart';

class DashboardPage extends StatefulWidget {
  const DashboardPage({super.key});

  @override
  State<DashboardPage> createState() => _DashboardPageState();
}

class _DashboardPageState extends State<DashboardPage> {
  @override
  void initState() {
    super.initState();
    // Refresh on load
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

  Color _getStatusColor(Status status) {
    return switch (status) {
      Status_Up() => Colors.green,
      Status_Created() => Colors.blue,
      Status_Exited() => Colors.orange,
      Status_Other() => Colors.grey,
    };
  }

  String _getStatusText(Status status) {
    return switch (status) {
      Status_Up(field0: final s) => 'Running${s.isNotEmpty ? ' $s' : ''}',
      Status_Created(field0: final s) => 'Created${s.isNotEmpty ? ' $s' : ''}',
      Status_Exited(field0: final s) => 'Exited${s.isNotEmpty ? ' $s' : ''}',
      Status_Other(field0: final s) => s.isNotEmpty ? s : 'Unknown',
    };
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(
        title: const Row(
          children: [
            Icon(Icons.dashboard, color: Color(0xFF137FEC)),
            SizedBox(width: 8),
            Text('Dashboard', style: TextStyle(fontWeight: FontWeight.bold)),
          ],
        ),
        actions: [
          IconButton(
            icon: const Icon(Icons.refresh),
            tooltip: 'Refresh',
            onPressed: () => context.read<AppStateProvider>().refresh(),
          ),
        ],
      ),
      body: Consumer<AppStateProvider>(
        builder: (context, appState, child) {
          if (appState.isLoading) {
            return const Center(child: CircularProgressIndicator());
          }

          if (!appState.isDistroboxInstalled) {
            return _buildDistroboxNotInstalledView(context, appState);
          }

          return RefreshIndicator(
            onRefresh: () => appState.refresh(),
            child: SingleChildScrollView(
              physics: const AlwaysScrollableScrollPhysics(),
              padding: const EdgeInsets.all(16),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  // Status Card
                  _buildStatusCard(context, appState),
                  const SizedBox(height: 16),

                  // Quick Stats Row
                  Row(
                    children: [
                      Expanded(
                        child: _buildStatCard(
                          context,
                          'Total Containers',
                          Icons.inventory_2,
                          '${appState.containers.length}',
                          Theme.of(context).colorScheme.primary,
                        ),
                      ),
                      const SizedBox(width: 12),
                      Expanded(
                        child: _buildStatCard(
                          context,
                          'Running',
                          Icons.play_circle,
                          '${appState.runningContainersCount}',
                          Colors.green,
                        ),
                      ),
                      const SizedBox(width: 12),
                      Expanded(
                        child: _buildStatCard(
                          context,
                          'Stopped',
                          Icons.stop_circle,
                          '${appState.stoppedContainersCount}',
                          Colors.orange,
                        ),
                      ),
                    ],
                  ),
                  const SizedBox(height: 24),

                  // Active Tasks Section
                  if (appState.activeTasks.isNotEmpty) ...[
                    _buildSectionHeader('Active Tasks'),
                    const SizedBox(height: 12),
                    ...appState.activeTasks.values.map((task) => Padding(
                          padding: const EdgeInsets.only(bottom: 8),
                          child: _buildTaskCard(context, task, appState),
                        )),
                    const SizedBox(height: 16),
                  ],

                  // Containers Section
                  Row(
                    mainAxisAlignment: MainAxisAlignment.spaceBetween,
                    children: [
                      _buildSectionHeader('Containers'),
                      TextButton(
                        onPressed: () {
                          // This would switch to containers tab in real app
                        },
                        child: const Text('View all'),
                      ),
                    ],
                  ),
                  const SizedBox(height: 12),
                  if (appState.containers.isEmpty)
                    _buildEmptyContainersCard(context)
                  else
                    ...appState.containers.take(5).map((container) => Padding(
                          padding: const EdgeInsets.only(bottom: 8),
                          child:
                              _buildContainerItem(context, container, appState),
                        )),

                  const SizedBox(height: 24),

                  // Quick Actions Section
                  _buildSectionHeader('Quick Actions'),
                  const SizedBox(height: 12),
                  _buildQuickActionsGrid(context, appState),

                  const SizedBox(height: 40),
                ],
              ),
            ),
          );
        },
      ),
    );
  }

  Widget _buildDistroboxNotInstalledView(
      BuildContext context, AppStateProvider appState) {
    return Center(
      child: Padding(
        padding: const EdgeInsets.all(32.0),
        child: Column(
          mainAxisAlignment: MainAxisAlignment.center,
          children: [
            Container(
              padding: const EdgeInsets.all(24),
              decoration: BoxDecoration(
                color: Colors.orange.withOpacity(0.1),
                shape: BoxShape.circle,
              ),
              child:
                  const Icon(Icons.warning_amber_rounded, size: 64, color: Colors.orange),
            ),
            const SizedBox(height: 24),
            const Text(
              'Distrobox Not Found',
              style: TextStyle(fontSize: 24, fontWeight: FontWeight.bold),
            ),
            const SizedBox(height: 12),
            Text(
              'Distrobox is required to manage Linux containers. Please install it to use Gosh Distrobox Manager.',
              textAlign: TextAlign.center,
              style: TextStyle(color: Theme.of(context).hintColor),
            ),
            const SizedBox(height: 24),
            FilledButton.icon(
              onPressed: () => appState.refresh(),
              icon: const Icon(Icons.refresh),
              label: const Text('Check Again'),
            ),
          ],
        ),
      ),
    );
  }

  Widget _buildStatusCard(BuildContext context, AppStateProvider appState) {
    final isHealthy = appState.error == null && appState.isDistroboxInstalled;
    final runningCount = appState.runningContainersCount;
    final totalCount = appState.containers.length;

    return Card(
      elevation: 0,
      shape: RoundedRectangleBorder(
        borderRadius: BorderRadius.circular(16),
        side: BorderSide(color: Theme.of(context).dividerColor),
      ),
      child: Padding(
        padding: const EdgeInsets.all(20),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Row(
              mainAxisAlignment: MainAxisAlignment.spaceBetween,
              children: [
                Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text(
                      'SYSTEM STATUS',
                      style: TextStyle(
                        color: Theme.of(context).colorScheme.primary,
                        fontSize: 12,
                        fontWeight: FontWeight.bold,
                        letterSpacing: 1.0,
                      ),
                    ),
                    const SizedBox(height: 4),
                    Text(
                      isHealthy ? 'All Systems Operational' : 'Attention Required',
                      style: const TextStyle(
                          fontSize: 20, fontWeight: FontWeight.bold),
                    ),
                  ],
                ),
                Container(
                  padding: const EdgeInsets.all(8),
                  decoration: BoxDecoration(
                    color: isHealthy
                        ? Colors.green.withOpacity(0.2)
                        : Colors.orange.withOpacity(0.2),
                    shape: BoxShape.circle,
                  ),
                  child: Icon(
                    isHealthy ? Icons.check_circle : Icons.warning,
                    color: isHealthy ? Colors.green : Colors.orange,
                  ),
                ),
              ],
            ),
            const SizedBox(height: 12),
            Text(
              totalCount == 0
                  ? 'No containers configured. Create one to get started!'
                  : '$runningCount of $totalCount container${totalCount != 1 ? 's' : ''} running.',
              style: TextStyle(color: Theme.of(context).hintColor),
            ),
            if (appState.error != null) ...[
              const SizedBox(height: 8),
              Container(
                padding: const EdgeInsets.all(8),
                decoration: BoxDecoration(
                  color: Colors.red.withOpacity(0.1),
                  borderRadius: BorderRadius.circular(8),
                ),
                child: Row(
                  children: [
                    const Icon(Icons.error_outline, color: Colors.red, size: 16),
                    const SizedBox(width: 8),
                    Expanded(
                      child: Text(
                        appState.error!,
                        style: const TextStyle(color: Colors.red, fontSize: 12),
                      ),
                    ),
                  ],
                ),
              ),
            ],
          ],
        ),
      ),
    );
  }

  Widget _buildStatCard(
    BuildContext context,
    String title,
    IconData icon,
    String value,
    Color color,
  ) {
    return Card(
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
            Icon(icon, size: 24, color: color),
            const SizedBox(height: 8),
            Text(
              value,
              style: TextStyle(
                fontSize: 28,
                fontWeight: FontWeight.bold,
                color: color,
              ),
            ),
            Text(
              title,
              style: TextStyle(
                fontSize: 12,
                color: Theme.of(context).hintColor,
              ),
            ),
          ],
        ),
      ),
    );
  }

  Widget _buildSectionHeader(String title) {
    return Text(
      title.toUpperCase(),
      style: TextStyle(
        fontSize: 12,
        fontWeight: FontWeight.bold,
        color: Theme.of(context).hintColor,
        letterSpacing: 1.0,
      ),
    );
  }

  Widget _buildTaskCard(
      BuildContext context, TaskInfo task, AppStateProvider appState) {
    final isRunning = !task.output.any((line) =>
        line.contains('completed') ||
        line.contains('failed') ||
        line.contains('Error'));

    return Card(
      elevation: 0,
      color: Theme.of(context).colorScheme.primary.withOpacity(0.05),
      shape: RoundedRectangleBorder(
        borderRadius: BorderRadius.circular(12),
        side: BorderSide(
            color: Theme.of(context).colorScheme.primary.withOpacity(0.2)),
      ),
      child: ListTile(
        leading: isRunning
            ? const SizedBox(
                width: 24,
                height: 24,
                child: CircularProgressIndicator(strokeWidth: 2),
              )
            : const Icon(Icons.check_circle, color: Colors.green),
        title: Text(task.description,
            style: const TextStyle(fontWeight: FontWeight.w500)),
        subtitle: Text(
          isRunning ? 'In progress...' : 'Completed',
          style: TextStyle(
            fontSize: 12,
            color: isRunning
                ? Theme.of(context).colorScheme.primary
                : Colors.green,
          ),
        ),
        trailing: isRunning
            ? IconButton(
                icon: const Icon(Icons.close, size: 20),
                onPressed: () => appState.cancelTask(task.id),
              )
            : null,
      ),
    );
  }

  Widget _buildContainerItem(
      BuildContext context, ContainerInfo container, AppStateProvider appState) {
    final statusColor = _getStatusColor(container.status);
    final statusText = _getStatusText(container.status);
    final isRunning = container.status is Status_Up;

    return Card(
      elevation: 0,
      shape: RoundedRectangleBorder(
        borderRadius: BorderRadius.circular(12),
        side: BorderSide(color: Theme.of(context).dividerColor),
      ),
      child: InkWell(
        borderRadius: BorderRadius.circular(12),
        onTap: () {
          appState.selectContainer(container);
          Navigator.of(context).push(
            MaterialPageRoute(
              builder: (context) => ContainerDetailsPage(container: container),
            ),
          );
        },
        child: Padding(
          padding: const EdgeInsets.all(12),
          child: Row(
            children: [
              Container(
                width: 40,
                height: 40,
                decoration: BoxDecoration(
                  color: Theme.of(context).colorScheme.surfaceContainerHighest,
                  borderRadius: BorderRadius.circular(10),
                ),
                child: Icon(
                  _getDistroIcon(container.image),
                  color: Theme.of(context).colorScheme.primary,
                  size: 20,
                ),
              ),
              const SizedBox(width: 12),
              Expanded(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text(container.name,
                        style: const TextStyle(
                            fontWeight: FontWeight.bold, fontSize: 14)),
                    Row(
                      children: [
                        Container(
                          width: 8,
                          height: 8,
                          decoration: BoxDecoration(
                            color: statusColor,
                            shape: BoxShape.circle,
                          ),
                        ),
                        const SizedBox(width: 6),
                        Text(
                          statusText,
                          style: TextStyle(
                            fontSize: 12,
                            color: statusColor,
                          ),
                        ),
                      ],
                    ),
                  ],
                ),
              ),
              if (isRunning)
                IconButton(
                  icon: const Icon(Icons.stop, size: 20),
                  color: Colors.orange,
                  tooltip: 'Stop',
                  onPressed: () => appState.stopContainer(container.name),
                ),
              const Icon(Icons.chevron_right, color: Colors.grey),
            ],
          ),
        ),
      ),
    );
  }

  Widget _buildEmptyContainersCard(BuildContext context) {
    return Card(
      elevation: 0,
      shape: RoundedRectangleBorder(
        borderRadius: BorderRadius.circular(12),
        side: BorderSide(color: Theme.of(context).dividerColor),
      ),
      child: Padding(
        padding: const EdgeInsets.all(24),
        child: Column(
          children: [
            Icon(Icons.inbox_outlined,
                size: 48, color: Theme.of(context).hintColor),
            const SizedBox(height: 12),
            Text(
              'No containers yet',
              style: TextStyle(
                fontWeight: FontWeight.bold,
                color: Theme.of(context).hintColor,
              ),
            ),
            const SizedBox(height: 4),
            Text(
              'Create your first container to get started',
              style: TextStyle(
                fontSize: 12,
                color: Theme.of(context).hintColor,
              ),
            ),
          ],
        ),
      ),
    );
  }

  Widget _buildQuickActionsGrid(BuildContext context, AppStateProvider appState) {
    return Wrap(
      spacing: 12,
      runSpacing: 12,
      children: [
        _buildQuickActionButton(
          context,
          Icons.add_circle_outline,
          'New Container',
          Colors.blue,
          () {
            // Navigate to create container
          },
        ),
        _buildQuickActionButton(
          context,
          Icons.system_update_alt,
          'Upgrade All',
          Colors.green,
          appState.runningContainersCount > 0
              ? () {
                  // Trigger upgrade all
                  ScaffoldMessenger.of(context).showSnackBar(
                    const SnackBar(
                        content: Text('Navigate to Updates page to upgrade')),
                  );
                }
              : null,
        ),
        _buildQuickActionButton(
          context,
          Icons.stop_circle_outlined,
          'Stop All',
          Colors.orange,
          appState.runningContainersCount > 0
              ? () async {
                  final confirmed = await showDialog<bool>(
                    context: context,
                    builder: (context) => AlertDialog(
                      title: const Text('Stop All Containers'),
                      content: Text(
                        'Stop all ${appState.runningContainersCount} running containers?',
                      ),
                      actions: [
                        TextButton(
                          onPressed: () => Navigator.pop(context, false),
                          child: const Text('Cancel'),
                        ),
                        FilledButton(
                          onPressed: () => Navigator.pop(context, true),
                          style: FilledButton.styleFrom(
                              backgroundColor: Colors.orange),
                          child: const Text('Stop All'),
                        ),
                      ],
                    ),
                  );
                  if (confirmed == true) {
                    await appState.stopAllContainers();
                  }
                }
              : null,
        ),
        _buildQuickActionButton(
          context,
          Icons.refresh,
          'Refresh',
          Theme.of(context).colorScheme.primary,
          () => appState.refresh(),
        ),
      ],
    );
  }

  Widget _buildQuickActionButton(
    BuildContext context,
    IconData icon,
    String label,
    Color color,
    VoidCallback? onPressed,
  ) {
    final isEnabled = onPressed != null;
    return InkWell(
      onTap: onPressed,
      borderRadius: BorderRadius.circular(12),
      child: Container(
        width: (MediaQuery.of(context).size.width - 56) / 2,
        padding: const EdgeInsets.all(16),
        decoration: BoxDecoration(
          color: isEnabled
              ? color.withOpacity(0.1)
              : Theme.of(context).disabledColor.withOpacity(0.05),
          borderRadius: BorderRadius.circular(12),
          border: Border.all(
            color: isEnabled
                ? color.withOpacity(0.3)
                : Theme.of(context).dividerColor,
          ),
        ),
        child: Row(
          children: [
            Icon(
              icon,
              color: isEnabled ? color : Theme.of(context).disabledColor,
            ),
            const SizedBox(width: 12),
            Expanded(
              child: Text(
                label,
                style: TextStyle(
                  fontWeight: FontWeight.w500,
                  color: isEnabled ? color : Theme.of(context).disabledColor,
                ),
              ),
            ),
          ],
        ),
      ),
    );
  }
}
